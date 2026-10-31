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

/// Native box identity; the uncolored texture is not the purple dye variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shulker {
    pub block_id: u32,
    pub texture: String,
    /// Java Direction order: down, up, north, south, west, east.
    pub facing: u8,
}
impl Shulker {
    pub fn from_state(state: &BlockState, block_id: u32) -> Result<Option<Self>, Error> {
        let name = state.name.strip_prefix("minecraft:").unwrap_or("");
        let suffix = if name == "shulker_box" {
            String::new()
        } else if let Some(color) = name.strip_suffix("_shulker_box") {
            if ![
                "white",
                "orange",
                "magenta",
                "light_blue",
                "yellow",
                "lime",
                "pink",
                "gray",
                "light_gray",
                "cyan",
                "purple",
                "blue",
                "brown",
                "green",
                "red",
                "black",
            ]
            .contains(&color)
            {
                return Ok(None);
            }
            format!("_{color}")
        } else {
            return Ok(None);
        };
        let facing = match state.properties.get("facing").map(String::as_str) {
            Some("down") => 0,
            Some("up") => 1,
            Some("north") => 2,
            Some("south") => 3,
            Some("west") => 4,
            Some("east") => 5,
            _ => {
                return Err(Error::Model(format!(
                    "invalid Java shulker properties for {}",
                    state.name
                )));
            }
        };
        Ok(Some(Self {
            block_id,
            texture: format!("textures/entity/shulker/shulker{suffix}"),
            facing,
        }))
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
    fn shulker_admission_requires_native_properties_and_texture_identity() {
        let mut state = BlockState {
            name: "minecraft:shulker_box".into(),
            properties: BTreeMap::new(),
            is_default: true,
        };
        assert!(Shulker::from_state(&state, 1).is_err());
        state.properties.insert("facing".into(), "up".into());
        assert_eq!(
            Shulker::from_state(&state, 1).unwrap().unwrap().texture,
            "textures/entity/shulker/shulker"
        );
        state.name = "minecraft:purple_shulker_box".into();
        assert_eq!(
            Shulker::from_state(&state, 2).unwrap().unwrap().texture,
            "textures/entity/shulker/shulker_purple"
        );
        state.name = "other:purple_shulker_box".into();
        assert!(Shulker::from_state(&state, 2).unwrap().is_none());
        state.name = "minecraft:shulker_box".into();
        state.properties.insert("facing".into(), "sideways".into());
        assert!(Shulker::from_state(&state, 1).is_err());
    }
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
        let shulkers: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| {
                Shulker::from_state(registry.state(id).unwrap(), registry.block_id(id).unwrap())
                    .unwrap()
                    .is_some()
            })
            .collect();
        assert_eq!(shulkers.len(), 102);
        let old = Arc::new(graphics);
        let extension = graphics::extend(Arc::clone(&old), &mut models, shulkers.clone()).unwrap();
        assert!(extension.extends(&old));
        let next = extension.graphics();
        assert_eq!(old.special_atlas.as_ref().unwrap().placements().len(), 19);
        assert_eq!(next.special_atlas.as_ref().unwrap().placements().len(), 36);
        for id in shulkers {
            assert_eq!(next.presentation(id), Some(graphics::Presentation::Shulker));
            let shape = &next.shulkers[&id];
            assert!(shape.facing < 6);
            let image = next
                .special_atlas
                .as_ref()
                .unwrap()
                .placement(&shape.texture)
                .unwrap();
            assert_eq!((image.width, image.height), (64, 64));
            if registry.state(id).unwrap().name.as_ref() == "minecraft:light_gray_shulker_box" {
                assert_eq!(shape.texture, "textures/entity/shulker/shulker_light_gray");
            }
        }
        let banners: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| {
                super::super::banner::Banner::from_state(registry.state(id).unwrap())
                    .unwrap()
                    .is_some()
            })
            .collect();
        assert_eq!(banners.len(), 320);
        let next = Arc::clone(next);
        let extension = graphics::extend(Arc::clone(&next), &mut models, banners.clone()).unwrap();
        let banner_graphics = extension.graphics();
        assert_eq!(banner_graphics.banner_patterns.len(), 43);
        assert_eq!(
            banner_graphics.banner_patterns["minecraft:diagonal_left"],
            "minecraft:textures/entity/banner/diagonal_left"
        );
        for id in banners {
            assert_eq!(
                banner_graphics.presentation(id),
                Some(graphics::Presentation::Banner)
            );
            assert!(banner_graphics.select(id, [-1, 16, 0]).unwrap().is_empty());
            let banner = &banner_graphics.banners[&id];
            let color_name = registry
                .state(id)
                .unwrap()
                .name
                .strip_prefix("minecraft:")
                .unwrap()
                .split("_wall_banner")
                .next()
                .unwrap()
                .trim_end_matches("_banner");
            assert!(
                super::super::banner::DYES
                    .iter()
                    .any(|(n, c)| *n == color_name && *c == banner.color)
            );
            assert!(banner.rotation < 16);
        }
        for texture in banner_graphics.banner_patterns.values() {
            let placement = banner_graphics
                .special_atlas
                .as_ref()
                .unwrap()
                .placement(texture)
                .unwrap();
            assert_eq!((placement.width, placement.height), (64, 64));
        }
        let skulls: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| {
                super::super::skull::Skull::from_state(registry.state(id).unwrap())
                    .unwrap()
                    .is_some()
            })
            .collect();
        assert_eq!(skulls.len(), 240);
        let extension =
            graphics::extend(Arc::clone(banner_graphics), &mut models, skulls.clone()).unwrap();
        let skull_graphics = extension.graphics();
        let mut textures = std::collections::BTreeSet::new();
        for id in skulls {
            assert_eq!(
                skull_graphics.presentation(id),
                Some(graphics::Presentation::Skull)
            );
            assert!(skull_graphics.select(id, [0, 0, 0]).unwrap().is_empty());
            let skull = &skull_graphics.skulls[&id];
            textures.insert(skull.texture);
            let p = skull_graphics
                .special_atlas
                .as_ref()
                .unwrap()
                .placement(skull.texture)
                .unwrap();
            let size = match skull.shape {
                super::super::skull::Shape::Dragon => (256, 256),
                super::super::skull::Shape::Cube => (64, 32),
                _ => (64, 64),
            };
            assert_eq!((p.width, p.height), size);
        }
        assert_eq!(textures.len(), 6);
        let statues: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| {
                super::super::statue::Statue::from_state(registry.state(id).unwrap())
                    .unwrap()
                    .is_some()
            })
            .collect();
        assert_eq!(statues.len(), 256);
        // No invisible admission when the required hierarchy is unavailable.
        assert!(graphics::extend(Arc::clone(skull_graphics), &mut models, [statues[0]]).is_err());
        models.entity_models = Some(Arc::new(
            crate::entity::java::Models::load(
                &std::fs::read(root.join("models.json")).unwrap(),
                hash("entity_models_report"),
                pin["entity_models_report"]["bytes"].as_u64().unwrap(),
            )
            .unwrap(),
        ));
        let extension =
            graphics::extend(Arc::clone(skull_graphics), &mut models, statues.clone()).unwrap();
        let g = extension.graphics();
        let mut textures = std::collections::BTreeSet::new();
        let mut poses = std::collections::BTreeSet::new();
        for id in statues {
            assert_eq!(g.presentation(id), Some(graphics::Presentation::Statue));
            let state = &g.statues[&id];
            textures.insert(state.texture);
            poses.insert(state.model);
            assert!(g.special_models.contains_key(state.model));
            let p = g
                .special_atlas
                .as_ref()
                .unwrap()
                .placement(state.texture)
                .unwrap();
            assert_eq!((p.width, p.height), (64, 64));
        }
        assert_eq!(textures.len(), 4);
        assert_eq!(poses.len(), 4);
        assert_eq!(g.special_models.len(), 4);
        let pots: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| registry.state(id).unwrap().name.as_ref() == "minecraft:decorated_pot")
            .collect();
        assert_eq!(pots.len(), 16);
        let extension = graphics::extend(Arc::clone(g), &mut models, pots.clone()).unwrap();
        let g = extension.graphics();
        assert_eq!(g.pot_patterns.len(), 23);
        assert!(
            g.special_models.contains_key("pot_base") && g.special_models.contains_key("pot_sides")
        );
        for id in pots {
            assert_eq!(g.presentation(id), Some(graphics::Presentation::Pot));
            assert!(g.pots.contains_key(&id));
        }
        for texture in g.pot_patterns.values() {
            let p = g
                .special_atlas
                .as_ref()
                .unwrap()
                .placement(texture)
                .unwrap();
            assert_eq!((p.width, p.height), (16, 16));
        }

        assert_eq!(g.special_models.len(), 6);
        let conduits: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| registry.state(id).unwrap().name.as_ref() == "minecraft:conduit")
            .collect();
        assert_eq!(conduits.len(), 2);
        let extension = graphics::extend(Arc::clone(g), &mut models, conduits.clone()).unwrap();
        let g = extension.graphics();
        for id in conduits {
            assert_eq!(g.presentation(id), Some(graphics::Presentation::Conduit));
            assert!(g.conduits.contains(&id));
        }
        let conduit = g.conduit.as_ref().unwrap();
        assert_eq!(conduit.parts.len(), 4);
        for wind in &conduit.winds {
            assert_eq!(wind.ticks, 3);
            assert_eq!(wind.frames.len(), 22);
            assert_eq!(wind.timeline, (0..22).collect::<Vec<_>>());
            for frame in &wind.frames {
                let p = g.special_atlas.as_ref().unwrap().placement(frame).unwrap();
                assert_eq!((p.width, p.height), (64, 32));
            }
        }
        let portals: Vec<_> = (0..registry.len() as u32)
            .filter(|&id| registry.state(id).unwrap().name.as_ref() == "minecraft:end_portal")
            .collect();
        assert_eq!(portals.len(), 1);
        let extension = graphics::extend(Arc::clone(g), &mut models, portals.clone()).unwrap();
        let g = extension.graphics();
        assert_eq!(
            g.presentation(portals[0]),
            Some(graphics::Presentation::Portal)
        );
        assert!(g.select(portals[0], [0; 3]).unwrap().is_empty());
        let palette = &g.portal.as_ref().unwrap().colors;
        assert_eq!(palette[0], [0.022087, 0.098399, 0.110818, 0.]);
        assert_eq!(palette[15], [0.080955, 0.314821, 0.661491, 0.]);
        for name in [super::super::portal::SKY, super::super::portal::STARS] {
            assert!(g.special_atlas.as_ref().unwrap().placement(name).is_some());
        }
    }
}
