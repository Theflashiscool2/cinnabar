//! Native Java banner states and data-driven pattern artwork.
use super::{
    BlockState,
    source::{Error, Identifier, Source},
    textures::Image,
};
use std::{collections::BTreeMap, sync::Arc};

/// Texture diffuse colors, checked against Java 26.3 DyeColor (not Bedrock numeric order).
pub const DYES: [(&str, u32); 16] = [
    ("white", 0xf9fffe),
    ("orange", 0xf9801d),
    ("magenta", 0xc74ebd),
    ("light_blue", 0x3ab3da),
    ("yellow", 0xfed83d),
    ("lime", 0x80c71f),
    ("pink", 0xf38baa),
    ("gray", 0x474f52),
    ("light_gray", 0x9d9d97),
    ("cyan", 0x169c9c),
    ("purple", 0x8932b8),
    ("blue", 0x3c44aa),
    ("brown", 0x835432),
    ("green", 0x5e7c16),
    ("red", 0xb02e26),
    ("black", 0x1d1d21),
];
pub const BASE: &str = "textures/entity/banner/banner_base";
pub const COLOR_BASE: &str = "textures/entity/banner/base";
#[derive(Clone, Debug, PartialEq)]
pub struct Banner {
    pub color: u32,
    pub wall: bool,
    /// Java rotation segment, clockwise around the world Y axis when viewed from above.
    pub rotation: u8,
}
impl Banner {
    pub fn from_state(state: &BlockState) -> Result<Option<Self>, Error> {
        let Some(name) = state.name.strip_prefix("minecraft:") else {
            return Ok(None);
        };
        let (color, wall) = if let Some(c) = name.strip_suffix("_wall_banner") {
            (c, true)
        } else if let Some(c) = name.strip_suffix("_banner") {
            (c, false)
        } else {
            return Ok(None);
        };
        let Some((_, color)) = DYES.iter().find(|(n, _)| *n == color) else {
            return Ok(None);
        };
        let invalid = || {
            Error::Model(format!(
                "invalid native Java banner properties: {}",
                state.name
            ))
        };
        let rotation = if wall {
            match state.properties.get("facing").map(String::as_str) {
                Some("south") => 0,
                Some("west") => 4,
                Some("north") => 8,
                Some("east") => 12,
                _ => return Err(invalid()),
            }
        } else {
            state
                .properties
                .get("rotation")
                .and_then(|s| s.parse::<u8>().ok())
                .filter(|n| *n < 16)
                .ok_or_else(invalid)?
        };
        Ok(Some(Self {
            color: *color,
            wall,
            rotation,
        }))
    }
}
/// Pattern registry keys resolve from the verified archive's own data, including aliases.
/// No Bedrock short codes or assumed pattern-to-filename table is used.
pub fn load(
    source: &mut dyn Source,
    images: &mut BTreeMap<String, Arc<Image>>,
) -> Result<BTreeMap<String, String>, Error> {
    let mut patterns = BTreeMap::new();
    let prefix = "data/minecraft/banner_pattern/";
    let entries = source.entries(prefix)?;
    if entries.len() > 256 {
        return Err(Error::Budget("Java banner patterns".into()));
    }
    let mut decoded_bytes = 0usize;
    for path in entries {
        let Some(name) = path
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(".json"))
        else {
            continue;
        };
        let data = source.json(&path)?;
        let asset = Identifier::parse(
            data.get("asset_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Model("Java banner pattern asset_id missing".into()))?,
        )?;
        let texture = format!(
            "{}:textures/entity/banner/{}",
            asset.namespace(),
            asset.path()
        );
        if !images.contains_key(&texture) {
            let bytes = source.read(&asset.asset_path("textures/entity/banner", "png"))?;
            let image = Image::decode(&bytes)?;
            decoded_bytes = decoded_bytes.saturating_add(image.rgba.len());
            if decoded_bytes > 64 * 1024 * 1024 {
                return Err(Error::Budget("Java banner pixels".into()));
            }
            images.insert(texture.clone(), Arc::new(image));
        }
        patterns.insert(format!("minecraft:{name}"), texture);
    }
    if patterns.is_empty() {
        return Err(Error::Model("Java banner pattern registry missing".into()));
    }
    for name in [BASE, COLOR_BASE] {
        images.insert(
            name.into(),
            Arc::new(Image::decode(
                &source.read(&format!("assets/minecraft/{name}.png"))?,
            )?),
        );
    }
    Ok(patterns)
}
