//! Cancellable native entity preparation. Files, PNG decoding and geometry work stay off-frame.
use assets::{
    entity::java::Models,
    registry::java::{
        source::{Identifier, Jar, Source},
        textures::Image,
    },
};
use render_model::{
    ActorArtworkLocation, ActorArtworkPages, ActorTexturePage,
    actor::{java_geometry::Model, java_model_rig_id},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use world::java::entities::visual::PigAppearance;

pub(super) struct Prepared {
    pub models: BTreeMap<String, Arc<Model>>,
    pub textures: BTreeMap<String, ActorArtworkLocation>,
    pub player_textures: BTreeMap<String, ActorArtworkLocation>,
    pub artwork: ActorArtworkPages,
}
enum Texture {
    Pig(String),
    Player(String),
}
pub(super) fn load(
    root: PathBuf,
    appearances: Vec<Arc<PigAppearance>>,
    cancel: Arc<AtomicBool>,
) -> Result<Prepared, String> {
    inner(root, appearances, cancel).map_err(|e| e.to_string())
}
fn inner(
    root: PathBuf,
    appearances: Vec<Arc<PigAppearance>>,
    cancel: Arc<AtomicBool>,
) -> Result<Prepared, Box<dyn std::error::Error>> {
    use java_protocol::target;
    let check = || {
        if cancel.load(Ordering::Acquire) {
            Err("Java entity preparation cancelled")
        } else {
            Ok(())
        }
    };
    check()?;
    let models = Models::load(
        &std::fs::read(root.join(target::ENTITY_MODELS_REPORT_PATH))?,
        target::ENTITY_MODELS_REPORT_SHA256,
        target::ENTITY_MODELS_REPORT_BYTES,
    )?;
    let mut compiled = BTreeMap::new();
    for (index, (name, part)) in models.iter().enumerate() {
        check()?;
        let id =
            java_model_rig_id(u32::try_from(index)?).ok_or("Java model GPU namespace exhausted")?;
        compiled.insert(name.to_owned(), Arc::new(Model::compile(part, id)?));
    }
    let mut jar = Jar::open(
        &root.join("client.jar"),
        target::CLIENT_ASSETS_SHA256,
        target::CLIENT_ASSETS_BYTES,
    )?;
    let keys = appearances
        .iter()
        .flat_map(|a| [a.adult_texture.to_string(), a.baby_texture.to_string()])
        .collect::<BTreeSet<_>>();
    let mut requests = Vec::new();
    for name in keys {
        let path = Identifier::parse(&name)?.asset_path("textures", "png");
        requests.push((Texture::Pig(name), path));
    }
    for skin in target::DEFAULT_PLAYER_SKINS {
        // ResourceTexture stores the complete resource path, unlike a pig variant's asset ID.
        let id = Identifier::parse(skin.texture)?;
        let path = format!("assets/{}/{}", id.namespace(), id.path());
        requests.push((Texture::Player(skin.texture.to_owned()), path));
    }
    let mut groups = BTreeMap::<(u16, u16), Vec<(Texture, Image)>>::new();
    for (name, path) in requests {
        check()?;
        let image = Image::decode(&jar.read(&path)?)?;
        groups
            .entry((u16::try_from(image.width)?, u16::try_from(image.height)?))
            .or_default()
            .push((name, image));
    }
    let mut pages = Vec::new();
    let mut textures = BTreeMap::new();
    let mut player_textures = BTreeMap::new();
    for ((width, height), images) in groups {
        // The shared page contract validates its finite GPU layer budget; no image is dropped.
        let page = u16::try_from(pages.len() + 1)?;
        let layers = u32::try_from(images.len())?;
        let mut rgba = Vec::new();
        for (layer, (name, image)) in images.into_iter().enumerate() {
            rgba.extend(image.rgba);
            let (map, name) = match name {
                Texture::Pig(name) => (&mut textures, name),
                Texture::Player(name) => (&mut player_textures, name),
            };
            map.insert(
                name,
                ActorArtworkLocation {
                    page,
                    layer: u32::try_from(layer)?,
                    pose_mode: assets::ActorPoseMode::CompiledLiteral,
                    multitexture: None,
                },
            );
        }
        pages.push(ActorTexturePage {
            width,
            height,
            layers,
            rgba8: rgba.into(),
            color_mask: false,
            multitexture: false,
        });
    }
    let routes = compiled
        .values()
        .map(|model| (model.geometry.id, 0, 0))
        .collect::<Vec<_>>();
    let artwork = ActorArtworkPages::from_entity_pages(pages, &routes)?;
    check()?;
    Ok(Prepared {
        models: compiled,
        textures,
        player_textures,
        artwork,
    })
}
