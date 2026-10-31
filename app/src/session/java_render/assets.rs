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
    pub interaction: Arc<assets::registry::java::interaction::Table>,
    pub physics: Arc<assets::registry::java::physics::Table>,
    pub graphics: Arc<Graphics>,
    pub tints: Vec<render_scene::chunk::biome_tints::BiomeTint>,
    pub unsupported_tints: BTreeSet<u32>,
}
struct CancelSource {
    jar: Jar,
    cancel: Arc<AtomicBool>,
}
pub(super) fn root() -> Result<PathBuf, String> {
    launcher::install_layout::discover_java_assets().map_err(|error| error.to_string())
}
/// Revalidates runtime inputs and decodes only newly requested models on the asset worker.
pub(super) fn extend(
    previous: Arc<Graphics>,
    states: BTreeSet<u32>,
    cancel: Arc<AtomicBool>,
) -> Result<graphics::Extension, String> {
    let work = || -> Result<_, Box<dyn std::error::Error>> {
        use java_protocol::target;
        if cancel.load(Ordering::Acquire) {
            return Err("Java asset load cancelled".into());
        }
        let root = root()?;
        let report = read_input(&root, "blocks.json")?;
        if report.len() != target::BLOCK_REPORT_BYTES {
            return Err("Java block report size does not match target manifest".into());
        }
        let registry = Arc::new(BlockRegistry::load(&report, target::BLOCK_REPORT_SHA256)?);
        let jar = Jar::open(
            &root.join("client.jar"),
            target::CLIENT_ASSETS_SHA256,
            target::CLIENT_ASSETS_BYTES,
        )?;
        let mut models = BlockModels::new(
            registry,
            Box::new(CancelSource {
                jar,
                cancel: Arc::clone(&cancel),
            }),
        );
        let extension = graphics::extend(previous, &mut models, states)
            .map_err(|error| model_error(error, &cancel))?;
        if cancel.load(Ordering::Acquire) {
            return Err("Java asset load cancelled".into());
        }
        Ok(extension)
    };
    work().map_err(|e| e.to_string())
}
impl Source for CancelSource {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
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
    let interaction_bytes = read_input(&root, "interaction.bin")?;
    if interaction_bytes.len() as u64 != target::INTERACTION_REPORT_BYTES {
        return Err("Java interaction report size does not match target manifest".into());
    }
    let interaction = Arc::new(assets::registry::java::interaction::Table::load(
        &interaction_bytes,
        target::INTERACTION_REPORT_SHA256,
        target::BLOCK_STATE_COUNT,
    )?);
    let physics_bytes = read_input(&root, "physics.bin")?;
    if physics_bytes.len() as u64 != target::PHYSICS_REPORT_BYTES {
        return Err("Java physics report size does not match target manifest".into());
    }
    let physics = Arc::new(assets::registry::java::physics::Table::load(
        &physics_bytes,
        target::PHYSICS_REPORT_SHA256,
        target::BLOCK_STATE_COUNT,
    )?);
    let report = read_input(&root, "blocks.json")?;
    if report.len() != target::BLOCK_REPORT_BYTES {
        return Err("Java block report size does not match target manifest".into());
    }
    let registry = Arc::new(BlockRegistry::load(&report, target::BLOCK_REPORT_SHA256)?);
    let jar = Jar::open(
        &root.join("client.jar"),
        target::CLIENT_ASSETS_SHA256,
        target::CLIENT_ASSETS_BYTES,
    )?;
    let mut source = CancelSource {
        jar,
        cancel: Arc::clone(&cancel),
    };
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
    let graphics = Arc::new(
        graphics::compile(
            &mut models,
            states,
            assets::MAX_CARRIER_TEXTURE_LAYERS as u32,
        )
        .map_err(|error| model_error(error, &cancel))?,
    );
    Ok(Prepared {
        interaction,
        physics,
        graphics,
        tints,
        unsupported_tints,
    })
}
fn read_input(root: &std::path::Path, name: &str) -> Result<Vec<u8>, String> {
    let path = root.join(name);
    std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Keeps model diagnostics separate from packet reports; no account or server data is included.
fn model_error(error: Error, cancel: &AtomicBool) -> String {
    if cancel.load(Ordering::Acquire) {
        return "Java asset load cancelled".into();
    }
    let Error::ModelBatch(ref report) = error else {
        return error.to_string();
    };
    let save = || -> Result<PathBuf, Box<dyn std::error::Error>> {
        use std::io::Write;
        let folder = launcher::install_layout::InstallLayout::discover()?.log_dir();
        std::fs::create_dir_all(&folder)?;
        let path = folder.join("java-models-latest.json");
        let document = serde_json::json!({
            "schema": 1,
            "scope": "observed_model_batch",
            "game_version": java_protocol::target::GAME_VERSION,
            "model_admission": report,
            "limitations": "First compiler failure per observed state; not all server packets or unvisited terrain. Use make java-audit for the whole pinned model registry."
        });
        let mut file = tempfile::NamedTempFile::new_in(folder)?;
        file.write_all(&serde_json::to_vec_pretty(&document)?)?;
        file.as_file().sync_all()?;
        if cancel.load(Ordering::Acquire) {
            return Err("Java asset load cancelled".into());
        }
        file.persist(&path)?;
        Ok(path)
    };
    match save() {
        Ok(path) => {
            bevy::log::warn!(path = %path.display(), failed_states = report.failed_states, "Java model audit saved");
            format!(
                "{error} Details: java-models-latest.json. This is a rendering support gap, not a request to regenerate assets."
            )
        }
        Err(write_error) => {
            bevy::log::warn!(%write_error, ?report, "Could not save Java model audit");
            format!("{error} Could not save model audit: {write_error}")
        }
    }
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
