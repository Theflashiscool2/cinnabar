//! Native conduit model and animated artwork admission.
use super::{
    BlockState,
    catalog::BlockModels,
    source::{Error, Identifier},
    textures::{Image, Sprite},
};
use std::{collections::BTreeMap, sync::Arc};
pub const MODELS: [&str; 4] = [
    "conduit_shell",
    "conduit_cage",
    "conduit_wind",
    "conduit_eye",
];
pub const TEXTURES: [&str; 6] = [
    "textures/entity/conduit/base",
    "textures/entity/conduit/cage",
    "textures/entity/conduit/wind",
    "textures/entity/conduit/wind_vertical",
    "textures/entity/conduit/open_eye",
    "textures/entity/conduit/closed_eye",
];
pub const EFFECT_TAG: &str = "minecraft:conduit_effect_block";
#[derive(Clone, Debug)]
pub struct Wind {
    pub frames: Vec<String>,
    pub timeline: Vec<usize>,
    pub ticks: u32,
}
#[derive(Clone, Debug)]
pub struct Resources {
    pub parts: [Arc<crate::entity::java::Part>; 4],
    pub winds: [Wind; 2],
}
pub fn matches(state: &BlockState) -> Result<bool, Error> {
    if state.name.as_ref() != "minecraft:conduit" {
        return Ok(false);
    }
    if !matches!(
        state.properties.get("waterlogged").map(String::as_str),
        Some("true" | "false")
    ) {
        return Err(Error::Model("invalid native conduit waterlogging".into()));
    }
    Ok(true)
}
pub fn load(
    models: &mut BlockModels,
    images: &mut BTreeMap<String, Arc<Image>>,
) -> Result<Resources, Error> {
    let parts = MODELS.map(|name| {
        models
            .entity_models
            .as_ref()
            .and_then(|m| m.get(name))
            .cloned()
            .map(Arc::new)
    });
    let [Some(shell), Some(cage), Some(wind), Some(eye)] = parts else {
        return Err(Error::Model("Java conduit hierarchy unavailable".into()));
    };
    for texture in [TEXTURES[0], TEXTURES[1], TEXTURES[4], TEXTURES[5]] {
        images.insert(
            texture.into(),
            Arc::new(Image::decode(
                &models
                    .source()
                    .read(&format!("assets/minecraft/{texture}.png"))?,
            )?),
        );
    }
    let mut winds = Vec::new();
    for texture in [TEXTURES[2], TEXTURES[3]] {
        let id = Identifier::parse(&format!(
            "minecraft:{}",
            texture.strip_prefix("textures/").expect("owned texture")
        ))?;
        let sprite = Sprite::load_entity(models.source(), &id)?;
        if sprite.interpolate {
            return Err(Error::Model(
                "interpolated Java conduit artwork is not supported".into(),
            ));
        }
        let mut frames = Vec::new();
        for (i, frame) in sprite.frames.into_iter().enumerate() {
            let key = format!("{texture}/frame_{i}");
            images.insert(key.clone(), Arc::new(frame));
            frames.push(key);
        }
        winds.push(Wind {
            frames,
            timeline: sprite.timeline,
            ticks: sprite.ticks,
        });
    }
    Ok(Resources {
        parts: [shell, cage, wind, eye],
        winds: winds.try_into().expect("two wind resources"),
    })
}
