//! Java special-renderer state and artwork, independent of Bedrock block/model identities.
use super::{BlockState, source::Error, textures::Image};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChestHalf {
    Single,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chest {
    pub block_id: u32,
    /// Identity includes waxing: unlike texture identity it governs double-chest pairing.
    pub block: Arc<str>,
    pub texture: String,
    /// South, west, north, east; quarter turns about the block center.
    pub turn: u8,
    pub half: ChestHalf,
}
impl Chest {
    pub fn from_state(state: &BlockState, block_id: u32) -> Result<Option<Self>, Error> {
        let name = state.name.strip_prefix("minecraft:").unwrap_or("");
        let bare = name.strip_prefix("waxed_").unwrap_or(name);
        let texture = match bare {
            "chest" => "normal",
            "trapped_chest" => "trapped",
            "ender_chest" => "ender",
            "copper_chest" => "copper",
            "exposed_copper_chest" => "copper_exposed",
            "weathered_copper_chest" => "copper_weathered",
            "oxidized_copper_chest" => "copper_oxidized",
            _ => return Ok(None),
        };
        let invalid = || Error::Model(format!("invalid Java chest properties for {}", state.name));
        let turn = match state.properties.get("facing").map(String::as_str) {
            Some("south") => 0,
            Some("west") => 1,
            Some("north") => 2,
            Some("east") => 3,
            _ => return Err(invalid()),
        };
        let half = match state.properties.get("type").map(String::as_str) {
            Some("single") => ChestHalf::Single,
            Some("left") => ChestHalf::Left,
            Some("right") => ChestHalf::Right,
            None if bare == "ender_chest" => ChestHalf::Single,
            _ => return Err(invalid()),
        };
        Ok(Some(Self {
            block_id,
            block: state.name.clone(),
            texture: texture.into(),
            turn,
            half,
        }))
    }
    pub fn texture_path(&self, half: ChestHalf) -> String {
        let suffix = match half {
            ChestHalf::Single => "",
            ChestHalf::Left => "_left",
            ChestHalf::Right => "_right",
        };
        format!("textures/entity/chest/{}{suffix}", self.texture)
    }
    pub fn partner_offset(&self) -> Option<[i32; 3]> {
        let sign = match self.half {
            ChestHalf::Single => return None,
            ChestHalf::Right => 1,
            ChestHalf::Left => -1,
        };
        Some(match self.turn {
            0 => [sign, 0, 0],
            1 => [0, 0, sign],
            2 => [-sign, 0, 0],
            _ => [0, 0, -sign],
        })
    }
    pub fn pairs_with(&self, other: &Self) -> bool {
        self.block == other.block
            && self.turn == other.turn
            && matches!(
                (self.half, other.half),
                (ChestHalf::Left, ChestHalf::Right) | (ChestHalf::Right, ChestHalf::Left)
            )
    }
}

/// Bounded shelf atlas for the shared block-entity GPU pass. Names and pixels determine identity.
pub fn atlas(
    images: &BTreeMap<String, Arc<Image>>,
) -> Result<Option<Arc<crate::RuntimeBlockEntityAssets>>, Error> {
    if images.is_empty() {
        return Ok(None);
    }
    let width = images.values().map(|i| i.width).max().unwrap().max(1024);
    if width > crate::MAX_BLOCK_ENTITY_ATLAS_SIDE {
        return Err(Error::Budget("Java special atlas width".into()));
    }
    let (mut x, mut y, mut row) = (0, 0, 0);
    let mut placements = Vec::new();
    for (name, image) in images {
        if x + image.width > width {
            x = 0;
            y += row;
            row = 0;
        }
        row = row.max(image.height);
        if y + row > crate::MAX_BLOCK_ENTITY_ATLAS_SIDE {
            return Err(Error::Budget("Java special atlas height".into()));
        }
        placements.push(crate::BlockEntityPlacement {
            name: name.clone().into(),
            x,
            y,
            width: image.width,
            height: image.height,
        });
        x += image.width;
    }
    let height = y + row;
    let mut pixels = vec![0; (width * height * 4) as usize];
    for (placement, image) in placements.iter().zip(images.values()) {
        for row in 0..image.height {
            let dest = ((placement.y + row) * width + placement.x) as usize * 4;
            let src = row as usize * image.width as usize * 4;
            pixels[dest..dest + image.width as usize * 4]
                .copy_from_slice(&image.rgba[src..src + image.width as usize * 4]);
        }
    }
    let names = serde_json::to_vec(&images.keys().collect::<Vec<_>>())
        .map_err(|e| Error::Model(e.to_string()))?;
    let bytes = crate::encode_block_entity_catalog(&names, width, height, &pixels, &placements)
        .map_err(|e| Error::Model(e.to_string()))?;
    crate::RuntimeBlockEntityAssets::decode(&bytes)
        .map(|a| Some(Arc::new(a)))
        .map_err(|e| Error::Model(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::java::{
        BlockRegistry, catalog::BlockModels, fluids::Table, graphics, source::Jar,
    };
    #[test]
    fn verified_chest_states_have_java_half_artwork_and_no_ordinary_model() {
        let Some(root) = std::env::var_os("CINNABAR_JAVA_TEST_ASSETS") else {
            eprintln!("SKIP verified Java assets unavailable");
            return;
        };
        let root = std::path::PathBuf::from(root);
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))
                .unwrap();
        let hash = |key: &str| pin[key]["sha256"].as_str().unwrap();
        let registry = Arc::new(
            BlockRegistry::load(
                &std::fs::read(root.join("blocks.json")).unwrap(),
                hash("block_registry_report"),
            )
            .unwrap(),
        );
        let mut selected = Vec::new();
        for id in 0..registry.len() as u32 {
            if let Some(chest) =
                Chest::from_state(registry.state(id).unwrap(), registry.block_id(id).unwrap())
                    .unwrap()
            {
                selected.push(id);
                assert!((0..4).contains(&chest.turn));
                if let Some(delta) = chest.partner_offset() {
                    let mut other = chest.clone();
                    other.half = match chest.half {
                        ChestHalf::Left => ChestHalf::Right,
                        _ => ChestHalf::Left,
                    };
                    assert!(chest.pairs_with(&other));
                    assert_eq!(other.partner_offset(), Some(delta.map(|n| -n)));
                    other.turn = (other.turn + 1) % 4;
                    assert!(!chest.pairs_with(&other));
                }
            }
        }
        assert_eq!(selected.len(), 248, "pinned 26.3 chest state coverage");
        assert_eq!(
            registry.block_id(registry.len() as u32 - 1),
            Some(1285),
            "verified BuiltInRegistries.BLOCK order"
        );
        let jar = Jar::open(
            &root.join("client.jar"),
            hash("client_assets"),
            pin["client_assets"]["bytes"].as_u64().unwrap(),
        )
        .unwrap();
        let mut models = BlockModels::new(registry.clone(), Box::new(jar));
        models.fluids = Some(Arc::new(
            Table::load(
                &std::fs::read(root.join("fluids.json")).unwrap(),
                hash("fluid_render_report"),
                registry.len(),
            )
            .unwrap(),
        ));
        let graphics = graphics::compile(&mut models, selected.clone(), 64).unwrap();
        let atlas = graphics.special_atlas.as_ref().unwrap();
        assert_eq!(atlas.placements().len(), 19); // six families x three layouts, plus ender single.
        for id in selected {
            assert_eq!(
                graphics.presentation(id),
                Some(graphics::Presentation::Chest)
            );
            assert!(graphics.select(id, [0; 3]).unwrap().is_empty());
            let chest = &graphics.chests[&id];
            let texture = atlas.placement(&chest.texture_path(chest.half)).unwrap();
            assert_eq!((texture.width, texture.height), (64, 64));
            if registry
                .state(id)
                .unwrap()
                .properties
                .get("waterlogged")
                .map(String::as_str)
                == Some("true")
            {
                assert!(
                    graphics
                        .fluid_materials(crate::registry::java::fluids::Kind::Water)
                        .is_some()
                );
            }
        }
    }
}
