//! Local hash-pinned Java asset loading on a cancellable worker.
use assets::registry::java::{
    BlockRegistry,
    catalog::BlockModels,
    graphics::{self, Graphics},
    source::{Error, Identifier, Jar, Source},
    textures::Image,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use world::java::{BiomeAppearance, Column};
pub(super) struct Prepared {
    pub graphics: Arc<Graphics>,
    pub tints: Vec<render_scene::chunk::biome_tints::BiomeTint>,
    pub unsupported_tints: BTreeSet<u32>,
}
struct CancelSource {
    jar: Jar,
    cancel: Arc<AtomicBool>,
}
impl Source for CancelSource {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(Error::Model("Java asset load cancelled".into()));
        }
        self.jar.read(path)
    }
}
pub(super) fn load(
    root: PathBuf,
    columns: Vec<Arc<Column>>,
    biomes: Vec<BiomeAppearance>,
    cancel: Arc<AtomicBool>,
) -> Result<Prepared, String> {
    load_inner(root, columns, biomes, cancel).map_err(|e| e.to_string())
}
fn load_inner(
    root: PathBuf,
    columns: Vec<Arc<Column>>,
    biomes: Vec<BiomeAppearance>,
    cancel: Arc<AtomicBool>,
) -> Result<Prepared, Box<dyn std::error::Error>> {
    use java_protocol::target;
    let report = std::fs::read(root.join("blocks.json"))?;
    if report.len() != target::BLOCK_REPORT_BYTES {
        return Err("Java block report size does not match target manifest".into());
    }
    let registry = Arc::new(BlockRegistry::load(&report, target::BLOCK_REPORT_SHA256)?);
    let jar = Jar::open(
        &root.join("client.jar"),
        target::CLIENT_ASSETS_SHA256,
        target::CLIENT_ASSETS_BYTES,
    )?;
    let mut source = CancelSource { jar, cancel };
    let mut colormap = |name: &str| -> Result<Image, Error> {
        Image::decode(&source.read(&Identifier::parse(name)?.asset_path("textures", "png"))?)
    };
    let grass = colormap("colormap/grass")?;
    let foliage = colormap("colormap/foliage")?;
    let mut tints = Vec::new();
    let mut unsupported_tints = BTreeSet::new();
    for (id, biome) in biomes.into_iter().enumerate() {
        if biome.grass_modifier.as_deref().is_some_and(|m| m != "none") {
            unsupported_tints.insert(id as u32);
        }
        let grass = biome
            .grass_color
            .unwrap_or(grass.climate_color(biome.temperature, biome.downfall)?);
        let foliage = biome
            .foliage_color
            .unwrap_or(foliage.climate_color(biome.temperature, biome.downfall)?);
        tints.push(render_scene::chunk::biome_tints::BiomeTint {
            grass: linear(grass),
            foliage: linear(foliage),
            water: linear(biome.water_color),
            ..Default::default()
        });
    }
    let states = columns
        .iter()
        .flat_map(|c| c.sections.iter())
        .flat_map(|s| (0..s.blocks.len()).filter_map(|i| s.blocks.get(i)))
        .map(|id| id.0)
        .collect::<BTreeSet<_>>();
    let mut models = BlockModels::new(registry, Box::new(source));
    // Baseline device array limit; the renderer validates the actual adapter before upload.
    let graphics = Arc::new(graphics::compile(
        &mut models,
        states,
        assets::MAX_CARRIER_TEXTURE_LAYERS as u32,
    )?);
    Ok(Prepared {
        graphics,
        tints,
        unsupported_tints,
    })
}
fn linear(rgb: [u8; 3]) -> [f32; 3] {
    rgb.map(|n| {
        let v = f32::from(n) / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}
