use super::*;
use assets::registry::java::{BlockRegistry, physics::climbing::Kind};
use std::{collections::BTreeMap, sync::Arc};
use world::java::{BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section};

pub(super) fn id(r: &BlockRegistry, name: &str, props: &[(&str, &str)]) -> u32 {
    r.state_id(
        name,
        &props
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap()
}
pub(super) fn scene(center: [i32; 3]) -> Terrain {
    let mut t = Terrain::default();
    t.begin(Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:climbing".into(),
        min_y: -64,
        height: 96,
        has_skylight: true,
    });
    for x in center[0].div_euclid(16) - 1..=center[0].div_euclid(16) + 1 {
        for z in center[2].div_euclid(16) - 1..=center[2].div_euclid(16) + 1 {
            t.replace(
                [x, z],
                Column::new(
                    (0..6)
                        .map(|_| {
                            Arc::new(Section {
                                blocks: Palette::uniform(BlockStateId(0), 4096),
                                biomes: Palette::uniform(BiomeId(0), 64),
                            })
                        })
                        .collect(),
                    Light {
                        sky: vec![None; 8],
                        block: vec![None; 8],
                    },
                    vec![],
                    vec![],
                ),
            );
        }
    }
    t
}
#[test]
fn native_scaffold_shapes_match84_official_context_vectors_across_partitions() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-climbing.json"
    ))
    .unwrap();
    for c in cases["scaffolding"].as_array().unwrap() {
        for offset in [[0, 0, 0], [-16, -16, -16], [16, 16, 16]] {
            let mut t = scene(offset);
            let state = c["state"].as_u64().unwrap() as u32;
            t.set_block(offset, BlockStateId(state));
            let ctx = collision::PlayerContext {
                bottom: f64::from(offset[1]) + c["bottom"].as_f64().unwrap(),
                descending: c["descending"].as_bool().unwrap(),
            };
            let region = Bounds {
                min: offset.map(|v| f64::from(v) - 0.1),
                max: offset.map(|v| f64::from(v) + 1.1),
            };
            let actual = collision::collect_player(&t, &p, (7, 1), region, ctx)
                .unwrap()
                .boxes;
            let expected: Vec<_> = c["boxes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| Bounds {
                    min: std::array::from_fn(|i| r[i].as_f64().unwrap() + f64::from(offset[i])),
                    max: std::array::from_fn(|i| r[i + 3].as_f64().unwrap() + f64::from(offset[i])),
                })
                .collect();
            assert_eq!(
                actual, expected,
                "state{state} bottom{} descending{} at {offset:?}",
                ctx.bottom, ctx.descending
            );
        }
    }
}
#[test]
fn climbing_uses_server_tag_ids_and_replacement_without_client_name_defaults() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let mut t = scene([8, 0, 8]);
    let ladder = id(r, "minecraft:ladder", &[("facing", "north")]);
    let native_tag = [r.block_id(ladder).unwrap() as i32];
    t.set_block([8, 0, 8], BlockStateId(ladder));
    let pos = [8.5, 0., 8.5];
    let mut b = Body::default();
    assert!(!super::super::climbing::at(pos, &mut b, &[], &t, &p).unwrap());
    assert!(super::super::climbing::at(pos, &mut b, &native_tag, &t, &p).unwrap());
    assert_eq!(b.last_climbable_position, Some([8, 0, 8]));
    assert!(!super::super::climbing::at(pos, &mut b, &[], &t, &p).unwrap());
    assert_eq!(b.last_climbable_position, Some([8, 0, 8]));
    // The server may tag an ordinary block; class names must not substitute for received tags.
    let stone = id(r, "minecraft:stone", &[]);
    t.set_block([8, 0, 8], BlockStateId(stone));
    assert!(
        super::super::climbing::at(pos, &mut b, &[r.block_id(stone).unwrap() as i32], &t, &p)
            .unwrap()
    );
    t.unload([0, 0]);
    let before = b.last_climbable_position;
    assert!(super::super::climbing::at(pos, &mut b, &native_tag, &t, &p).is_err());
    assert_eq!(b.last_climbable_position, before);
}
#[test]
fn all_native_trapdoor_classes_require_open_and_matching_ladder_below() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-climbing.json"
    ))
    .unwrap();
    for name in cases["classes"]["trapdoor"].as_array().unwrap() {
        for facing in ["north", "south", "west", "east"] {
            for open in ["false", "true"] {
                let mut t = scene([8, 0, 8]);
                let trap = id(
                    r,
                    name.as_str().unwrap(),
                    &[("facing", facing), ("open", open)],
                );
                assert_eq!(
                    assets::registry::java::physics::climbing::kind(r, trap),
                    Some(Kind::Trapdoor)
                );
                t.set_block([8, 1, 8], BlockStateId(trap));
                let mut b = Body::default();
                for below in [facing, if facing == "north" { "south" } else { "north" }] {
                    t.set_block(
                        [8, 0, 8],
                        BlockStateId(id(r, "minecraft:ladder", &[("facing", below)])),
                    );
                    assert_eq!(
                        super::super::climbing::at([8.5, 1., 8.5], &mut b, &[], &t, &p).unwrap(),
                        open == "true" && facing == below,
                        "{name}/{facing}/{open}/{below}"
                    );
                }
            }
        }
    }
}
#[test]
fn native_ladder_slide_clamps_preserve_float_widening_and_scaffold_descent() {
    for (shift, scaffold, y) in [
        (false, false, -0.15000000596046448),
        (true, false, 0.),
        (true, true, -0.15000000596046448),
    ] {
        let mut v = [1., -1., -1.];
        super::super::climbing::clamp(&mut v, shift, scaffold);
        assert_eq!(v, [0.15000000596046448, y, -0.15000000596046448]);
    }
    let mut v = [0., 0.42, 0.];
    super::super::climbing::clamp(&mut v, true, false);
    assert_eq!(v, [0., 0.42, 0.]);
}
#[test]
fn ladder_jump_velocity_is_applied_after_move_and_authoritative_removal_releases_it() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let ladder = id(r, "minecraft:ladder", &[("facing", "north")]);
    let members = [r.block_id(ladder).unwrap() as i32];
    let mut parameters = terrain::parameters();
    parameters.climbable = &members;
    let mut t = scene([8, 0, 8]);
    for y in 0..6 {
        t.set_block([8, y, 8], BlockStateId(ladder));
    }
    let mut m = Motion {
        position: [8.5, 2., 8.4],
        ..Default::default()
    };
    let mut b = Body::default();
    let input = Input {
        jump: true,
        ..Default::default()
    };
    step(&mut m, &mut b, input, parameters, &t, &p, (7, 1)).unwrap();
    assert_eq!(m.position[1], 2.);
    assert_eq!(m.velocity[1], (0.2 - 0.08) * f64::from(0.98_f32));
    assert_eq!(b.last_climbable_position, Some([8, 2, 8]));
    step(&mut m, &mut b, input, parameters, &t, &p, (7, 1)).unwrap();
    assert!(m.position[1] > 2.1);
    for y in 0..6 {
        t.set_block([8, y, 8], BlockStateId(0));
    }
    m.velocity = [0.; 3];
    step(&mut m, &mut b, input, parameters, &t, &p, (7, 1)).unwrap();
    assert_eq!(m.velocity[1], -0.08 * f64::from(0.98_f32));
}

#[test]
fn scaffold_platform_lands_then_shift_descends_and_authoritative_removal_releases_support() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let scaffold = id(
        r,
        "minecraft:scaffolding",
        &[("distance", "0"), ("bottom", "false")],
    );
    let stone = id(r, "minecraft:stone", &[]);
    let members = [r.block_id(scaffold).unwrap() as i32];
    let mut params = terrain::parameters();
    params.climbable = &members;
    let mut t = scene([8, 0, 8]);
    t.set_block([8, -1, 8], BlockStateId(stone));
    for y in 0..3 {
        t.set_block([8, y, 8], BlockStateId(scaffold));
    }
    let mut m = Motion {
        position: [8.5, 3.5, 8.5],
        velocity: [0., -0.2, 0.],
        ..Default::default()
    };
    let mut b = Body::default();
    for _ in 0..8 {
        step(&mut m, &mut b, Input::default(), params, &t, &p, (7, 1)).unwrap();
    }
    assert_eq!(m.position[1], 3.);
    assert!(b.on_ground);
    let mut falling = m;
    let mut fall_body = b;
    for y in 0..3 {
        t.set_block([8, y, 8], BlockStateId(0));
    }
    step(
        &mut falling,
        &mut fall_body,
        Input::default(),
        params,
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(falling.position[1] < 3.);
    assert!(!fall_body.on_ground);
    for y in 0..3 {
        t.set_block([8, y, 8], BlockStateId(scaffold));
    }
    for _ in 0..30 {
        step(
            &mut m,
            &mut b,
            Input {
                crouch: true,
                ..Default::default()
            },
            params,
            &t,
            &p,
            (7, 1),
        )
        .unwrap();
    }
    assert_eq!(m.position[1], 0.);
    assert!(b.on_ground && b.crouched);
    assert_eq!(b.last_climbable_position, Some([8, 0, 8]));
}

#[test]
fn waterlogged_ladder_uses_water_wall_ascent_and_dry_ladder_sneak_holds_height() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let mut t = scene([8, 0, 8]);
    let ladder = id(
        r,
        "minecraft:ladder",
        &[("facing", "north"), ("waterlogged", "true")],
    );
    let members = [r.block_id(ladder).unwrap() as i32];
    let mut params = terrain::parameters();
    params.climbable = &members;
    for y in 0..6 {
        t.set_block([8, y, 8], BlockStateId(ladder));
    }
    let wall = id(r, "minecraft:stone", &[]);
    for y in 0..6 {
        t.set_block([8, y, 9], BlockStateId(wall));
    }
    let mut m = Motion {
        position: [8.5, 2., 8.5],
        velocity: [0., 0., 0.2],
        ..Default::default()
    };
    let mut b = Body::default();
    step(
        &mut m,
        &mut b,
        Input {
            forward: 1.,
            ..Default::default()
        },
        params,
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(b.horizontal_collision);
    assert_eq!(m.velocity[1], 0.2 * f64::from(0.8_f32) - 0.005);
    let dry = id(
        r,
        "minecraft:ladder",
        &[("facing", "north"), ("waterlogged", "false")],
    );
    for y in 0..6 {
        t.set_block([8, y, 8], BlockStateId(dry));
    }
    m.position = [8.5, 2., 8.4];
    m.velocity = [0.; 3];
    for _ in 0..8 {
        step(
            &mut m,
            &mut b,
            Input {
                crouch: true,
                ..Default::default()
            },
            params,
            &t,
            &p,
            (7, 1),
        )
        .unwrap();
    }
    assert_eq!(m.position[1], 2.);
    assert!(b.crouched);
}
