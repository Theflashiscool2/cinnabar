//! Native decorated-pot states and artwork from the pinned Java resource registry.
use super::{
    BlockState,
    source::{Error, Identifier, Source},
    textures::Image,
};
use std::{collections::BTreeMap, sync::Arc};

pub const BASE_PARTS: &[&str] = &["neck", "top", "bottom"];
pub const SIDE_PARTS: &[&str] = &["back", "left", "right", "front"];
/// Pinned default item component correspondence, checked against all native initializers.
pub fn default_pattern(id: &Identifier) -> Option<String> {
    id.path()
        .strip_suffix("_pottery_sherd")
        .map(|p| format!("{}:{p}", id.namespace()))
}
pub const BASE: &str = "minecraft:textures/entity/decorated_pot/decorated_pot_base";
pub const SIDE: &str = "minecraft:textures/entity/decorated_pot/decorated_pot_side";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pot {
    pub block_id: u32,
    /// South, west, north, east.
    pub turn: u8,
}
impl Pot {
    pub fn from_state(state: &BlockState, block_id: u32) -> Result<Option<Self>, Error> {
        if state.name.as_ref() != "minecraft:decorated_pot" {
            return Ok(None);
        }
        let invalid = || Error::Model("invalid native decorated-pot properties".into());
        let turn = match state.properties.get("facing").map(String::as_str) {
            Some("south") => 0,
            Some("west") => 1,
            Some("north") => 2,
            Some("east") => 3,
            _ => return Err(invalid()),
        };
        for name in ["waterlogged", "cracked"] {
            if !matches!(
                state.properties.get(name).map(String::as_str),
                Some("true" | "false")
            ) {
                return Err(invalid());
            }
        }
        Ok(Some(Self { block_id, turn }))
    }
}

/// Pattern holders use registry data, not Bedrock sherd indices or texture paths from NBT.
pub fn load(
    source: &mut dyn Source,
    images: &mut BTreeMap<String, Arc<Image>>,
) -> Result<BTreeMap<String, String>, Error> {
    let prefix = "data/minecraft/decorated_pot_pattern/";
    let entries = source.entries(prefix)?;
    if entries.len() > 256 {
        return Err(Error::Budget("Java pottery patterns".into()));
    }
    let mut patterns = BTreeMap::new();
    for path in entries {
        let Some(name) = path
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(".json"))
        else {
            continue;
        };
        let data = source.json(&path)?;
        let id = Identifier::parse(
            data.get("asset_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Model("Java pottery asset_id missing".into()))?,
        )?;
        patterns.insert(
            format!("minecraft:{name}"),
            format!(
                "{}:textures/entity/decorated_pot/{}",
                id.namespace(),
                id.path()
            ),
        );
    }
    let mut total = 0usize;
    for texture in patterns.values().map(String::as_str).chain([BASE, SIDE]) {
        if images.contains_key(texture) {
            continue;
        }
        let id = Identifier::parse(texture)?;
        let image = Image::decode(&source.read(&format!(
            "assets/{}/{}.png",
            id.namespace(),
            id.path()
        ))?)?;
        total = total.saturating_add(image.rgba.len());
        if total > 16 * 1024 * 1024 {
            return Err(Error::Budget("Java pottery artwork".into()));
        }
        images.insert(texture.into(), Arc::new(image));
    }
    Ok(patterns)
}
