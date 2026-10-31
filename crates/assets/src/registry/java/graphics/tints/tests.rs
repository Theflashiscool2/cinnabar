use super::*;
use crate::registry::java::{BlockRegistry, catalog::BlockModels, graphics, source::Jar};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

fn state(name: &str, props: &[(&str, &str)]) -> BlockState {
    BlockState {
        name: format!("minecraft:{name}").into(),
        properties: props
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        is_default: true,
    }
}

fn pin() -> serde_json::Value {
    let pin: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    assert_eq!(
        pin["game_version"], "26.3",
        "recheck BlockColors registrations on target updates"
    );
    pin
}

#[test]
fn registered_layers_are_distinct_from_arbitrary_model_tint_indices() {
    pin();
    for name in [
        "bamboo",
        "bamboo_sapling",
        "cherry_leaves",
        "pale_oak_leaves",
        "stonecutter",
        "lava_cauldron",
        "powder_snow_cauldron",
    ] {
        assert_eq!(
            material(&state(name, &[]), 0).unwrap(),
            0,
            "{name} has no registered BlockColors callback"
        );
    }
    for name in ["pink_petals", "wildflowers"] {
        assert_eq!(material(&state(name, &[]), 0).unwrap(), 0);
        assert_eq!(
            material(&state(name, &[]), 1).unwrap(),
            MATERIAL_FLAG_GRASS_TINT
        );
        assert_eq!(material(&state(name, &[]), 2).unwrap(), 0);
    }
    for (name, flags) in [
        ("sugar_cane", MATERIAL_FLAG_GRASS_TINT),
        ("oak_leaves", MATERIAL_FLAG_FOLIAGE_TINT),
        ("mangrove_leaves", MATERIAL_FLAG_FOLIAGE_TINT),
        ("water_cauldron", MATERIAL_FLAG_WATER_TINT),
        (
            "leaf_litter",
            MATERIAL_FLAG_FOLIAGE_TINT | MATERIAL_FLAG_DRY_FOLIAGE,
        ),
        (
            "birch_leaves",
            MATERIAL_FLAG_FOLIAGE_TINT | MATERIAL_FLAG_BIRCH_FOLIAGE,
        ),
        (
            "spruce_leaves",
            MATERIAL_FLAG_FOLIAGE_TINT | MATERIAL_FLAG_EVERGREEN_FOLIAGE,
        ),
    ] {
        assert_eq!(material(&state(name, &[]), 0).unwrap(), flags);
        assert!(crate::compiled::material_flags_are_valid(flags));
        assert_eq!(material(&state(name, &[]), -1).unwrap(), 0);
        assert_eq!(material(&state(name, &[]), 1).unwrap(), 0);
    }
    assert_eq!(BIRCH, [128, 167, 85]);
    assert_eq!(SPRUCE, [97, 153, 97]);
}

#[test]
fn stem_colors_follow_authoritative_age_and_reject_malformed_states() {
    for name in ["melon_stem", "pumpkin_stem"] {
        for (age, expected) in [
            ("0", [0, 255, 0]),
            ("3", [96, 231, 12]),
            ("7", [224, 199, 28]),
        ] {
            let flags = material(&state(name, &[("age", age)]), 0).unwrap();
            assert_eq!(crate::stem::material_colour(flags), Some(expected));
            assert!(crate::compiled::material_flags_are_valid(flags));
        }
        for props in [vec![], vec![("age", "8")], vec![("age", "-1")]] {
            assert!(material(&state(name, &props), 0).is_err());
        }
    }
    assert_eq!(
        crate::stem::material_colour(material(&state("attached_melon_stem", &[]), 0).unwrap()),
        Some([224, 199, 28])
    );
}

#[test]
fn intentional_invisibility_never_admits_fluids_or_special_renderers() {
    for name in ["barrier", "light"] {
        assert!(allows_empty_model(&state(
            name,
            &[("waterlogged", "false")]
        )));
        assert!(!allows_empty_model(&state(
            name,
            &[("waterlogged", "true")]
        )));
        assert!(!allows_empty_model(&state(name, &[])));
    }
    for age in ["0", "1", "2"] {
        assert!(allows_empty_model(&state(
            "pitcher_crop",
            &[("half", "upper"), ("age", age)]
        )));
        assert!(!allows_empty_model(&state(
            "pitcher_crop",
            &[("half", "lower"), ("age", age)]
        )));
    }
    assert!(!allows_empty_model(&state(
        "pitcher_crop",
        &[("half", "upper"), ("age", "3")]
    )));
    for name in [
        "lava",
        "water",
        "bubble_column",
        "chest",
        "ender_chest",
        "end_portal",
        "moving_piston",
    ] {
        assert!(!allows_empty_model(&state(name, &[])));
    }
}

/// Actual official resource traversal, with synthetic IDs/properties (not server qualification).
#[test]
fn pinned_resources_compile_tints_and_empty_models_without_erasing_real_geometry() {
    let Some(path) = std::env::var_os("CINNABAR_JAVA_CLIENT_TEST") else {
        eprintln!("SKIP: missing CINNABAR_JAVA_CLIENT_TEST pinned client.jar fixture");
        return;
    };
    let pin = pin();
    let jar = Jar::open(
        std::path::Path::new(&path),
        pin["client_assets"]["sha256"].as_str().unwrap(),
        pin["client_assets"]["bytes"].as_u64().unwrap(),
    )
    .unwrap();
    let cases = [
        state("oak_leaves", &[]),
        state("birch_leaves", &[]),
        state("spruce_leaves", &[]),
        state("cherry_leaves", &[]),
        state("pale_oak_leaves", &[]),
        state("sugar_cane", &[]),
        state("melon_stem", &[("age", "0")]),
        state("pumpkin_stem", &[("age", "7")]),
        state("water_cauldron", &[("level", "1")]),
        state("lava_cauldron", &[]),
        state("powder_snow_cauldron", &[("level", "3")]),
        state(
            "pink_petals",
            &[("facing", "north"), ("flower_amount", "4")],
        ),
        state("wildflowers", &[("facing", "east"), ("flower_amount", "4")]),
        state(
            "leaf_litter",
            &[("facing", "south"), ("segment_amount", "4")],
        ),
        state("stonecutter", &[("facing", "west")]),
        state("bamboo_sapling", &[]),
        state("tall_grass", &[("half", "lower")]),
        state("tall_grass", &[("half", "upper")]),
        state("lily_pad", &[]),
        state(
            "redstone_wire",
            &[
                ("power", "15"),
                ("north", "none"),
                ("south", "none"),
                ("east", "none"),
                ("west", "none"),
            ],
        ),
        state("pitcher_crop", &[("age", "3"), ("half", "upper")]),
        state("structure_void", &[]),
        state("barrier", &[("waterlogged", "false")]),
        state("light", &[("waterlogged", "false"), ("level", "0")]),
        state("acacia_leaves", &[]),
        state("dark_oak_leaves", &[]),
        state("jungle_leaves", &[]),
        state("mangrove_leaves", &[]),
        state(
            "vine",
            &[
                ("north", "true"),
                ("south", "false"),
                ("east", "false"),
                ("west", "false"),
                ("up", "false"),
            ],
        ),
        state("attached_melon_stem", &[("facing", "north")]),
        state("attached_pumpkin_stem", &[("facing", "east")]),
        state(
            "bamboo",
            &[("age", "1"), ("leaves", "large"), ("stage", "0")],
        ),
    ];
    let mut report = BTreeMap::<String, serde_json::Value>::new();
    for (id, state) in cases.iter().enumerate() {
        let entry = report
            .entry(state.name.to_string())
            .or_insert_with(|| json!({"properties":{},"states":[]}));
        for (key, value) in &state.properties {
            let values = entry["properties"]
                .as_object_mut()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap();
            if !values.contains(&json!(value)) {
                values.push(json!(value));
            }
        }
        let states = entry["states"].as_array_mut().unwrap();
        states.push(json!({"id":id,"default":states.is_empty(),"properties":state.properties}));
    }
    let bytes = serde_json::to_vec(&report).unwrap();
    let registry = BlockRegistry::load(&bytes, &format!("{:x}", Sha256::digest(&bytes))).unwrap();
    let mut models = BlockModels::new(Arc::new(registry), Box::new(jar));
    let compiled = graphics::compile(&mut models, 0..cases.len() as u32, 64).unwrap();
    for (id, state) in cases.iter().enumerate() {
        let selected = compiled.select(id as u32, [-1, 63, 16]).unwrap();
        assert_eq!(
            selected.is_empty(),
            allows_empty_model(state),
            "{}",
            state.name
        );
    }
    // Petals retain untinted flowers and a separately biome-tinted stem layer.
    let flags = compiled
        .select(
            cases
                .iter()
                .position(|s| &*s.name == "minecraft:pink_petals")
                .unwrap() as u32,
            [0; 3],
        )
        .unwrap()
        .iter()
        .flat_map(|id| {
            let template = compiled.runtime.model_templates()[*id as usize];
            compiled.runtime.model_quads()
                [template.quad_start as usize..(template.quad_start + template.quad_count) as usize]
                .iter()
                .map(|q| {
                    compiled.runtime.material(q.material).flags & crate::MATERIAL_FLAG_TINT_MASK
                })
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(flags, [0, MATERIAL_FLAG_GRASS_TINT].into());
}

#[test]
fn power_lily_and_tall_plant_callbacks_preserve_layer_and_position_semantics() {
    pin();
    assert_eq!(LILY, [32, 128, 48]);
    for (power, expected) in [
        (0, [76, 0, 0]),
        (1, [112, 0, 0]),
        (7, [173, 0, 0]),
        (15, [255, 50, 0]),
    ] {
        assert_eq!(redstone(power), Some(expected));
        let s = state("redstone_wire", &[("power", &power.to_string())]);
        let flags = quad_flags(&s, 0).unwrap();
        assert_eq!(
            flags,
            crate::model::MODEL_QUAD_JAVA_TINT_REDSTONE
                | u32::from(power) << crate::model::MODEL_QUAD_JAVA_POWER_SHIFT
        );
        assert!(crate::model::model_quad_flags_are_valid(flags));
        assert_eq!(material(&s, 0).unwrap(), 0);
        assert_eq!(quad_flags(&s, 1).unwrap(), 0);
    }
    assert_eq!(redstone(16), None);
    assert!(quad_flags(&state("redstone_wire", &[("power", "16")]), 0).is_err());
    assert_eq!(
        quad_flags(&state("lily_pad", &[]), 0).unwrap(),
        crate::model::MODEL_QUAD_JAVA_TINT_LILY
    );
    for name in ["tall_grass", "large_fern"] {
        assert_eq!(
            quad_flags(&state(name, &[("half", "upper")]), 0).unwrap(),
            crate::model::MODEL_QUAD_JAVA_TINT_BELOW
        );
        assert_eq!(
            quad_flags(&state(name, &[("half", "lower")]), 0).unwrap(),
            0
        );
        assert_eq!(
            material(&state(name, &[("half", "upper")]), 0).unwrap(),
            MATERIAL_FLAG_GRASS_TINT
        );
    }
}
