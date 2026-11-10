use super::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use world::java::{BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section};
fn scene() -> (Terrain, Table) {
    let mut bytes = b"CBJPHYS1".to_vec();
    bytes.extend(6u32.to_be_bytes());
    for id in 0..6u32 {
        bytes.extend(id.to_be_bytes());
        bytes.push(0);
        for n in [if id == 5 { 0.98_f32 } else { 0.6_f32 }, 1., 1., 0.] {
            bytes.extend(n.to_be_bytes());
        }
        bytes.extend([u8::from(id == 0), u8::from(id != 2)]);
        let fluid = if id == 2 {
            "minecraft:water"
        } else {
            "minecraft:empty"
        };
        bytes.extend((fluid.len() as u16).to_be_bytes());
        bytes.extend(fluid.bytes());
        bytes.extend(u32::from(matches!(id, 1 | 3 | 4 | 5)).to_be_bytes());
        if matches!(id, 1 | 3 | 4 | 5) {
            let height = match id {
                3 => 0.5,
                4 => 1.5,
                _ => 1.,
            };
            for n in [0_f64, 0., 0., 1., height, 1.] {
                bytes.extend(n.to_be_bytes());
            }
        }
    }
    let table = Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 6).unwrap();
    let mut terrain = Terrain::default();
    terrain.begin(Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:dry-item".into(),
        min_y: -16,
        height: 32,
        has_skylight: true,
    });
    terrain.replace(
        [0, 0],
        Column::new(
            [1, 0]
                .into_iter()
                .map(|id| {
                    Arc::new(Section {
                        blocks: Palette::uniform(BlockStateId(id), 4096),
                        biomes: Palette::uniform(BiomeId(0), 64),
                    })
                })
                .collect(),
            Light {
                sky: vec![None; 4],
                block: vec![None; 4],
            },
            vec![],
            vec![],
        ),
    );
    (terrain, table)
}
#[test]
fn native_server_tick_reference_includes_flight_landing_and_resting_cadence() {
    let (terrain, physics) = scene();
    let data: serde_json::Value =
        serde_json::from_str(include_str!("native-dry-vectors.json")).unwrap();
    let rows = data["ticks"].as_array().unwrap();
    let mut view = View::new(
        world::java::player::Motion {
            position: [8.5, 4., 8.5],
            velocity: [0.1, 0.2, 0.],
            ..Default::default()
        },
        [0.25; 2],
    );
    for (i, row) in rows.iter().enumerate() {
        if i != 0 {
            tick(
                &terrain,
                &physics,
                (7, 1),
                &mut view,
                data["network_id"].as_i64().unwrap() as i32,
                false,
            )
            .unwrap();
        }
        for axis in 0..3 {
            assert!(
                (view.position[axis] - row["position"][axis].as_f64().unwrap()).abs() < 1e-12,
                "tick {i} position {axis}: {:?}",
                view.position
            );
            assert!(
                (view.velocity[axis] - row["velocity"][axis].as_f64().unwrap()).abs() < 1e-12,
                "tick {i} velocity {axis}: {:?}",
                view.velocity
            );
        }
        assert_eq!(
            view.on_ground,
            row["on_ground"].as_bool().unwrap(),
            "tick {i}"
        );
    }
}
#[test]
fn unavailable_fluid_and_retired_queries_do_not_commit_motion() {
    let (mut terrain, physics) = scene();
    let mut view = View::new(
        world::java::player::Motion {
            position: [8.5, 2., 8.5],
            velocity: [0.1, 0.2, 0.],
            ..Default::default()
        },
        [0.25; 2],
    );
    let original = view.clone();
    assert!(tick(&terrain, &physics, (7, 2), &mut view, 1, false).is_err());
    assert_eq!(view.position, original.position);
    assert_eq!(view.velocity, original.velocity);
    terrain.set_block([8, 2, 8], BlockStateId(2));
    assert!(matches!(
        tick(&terrain, &physics, (7, 1), &mut view, 1, false),
        Err(Error::Unsupported)
    ));
    assert_eq!(view.position, original.position);
    view.position = [30., 2., 30.];
    assert!(tick(&terrain, &physics, (7, 1), &mut view, 1, false).is_err());
}
#[test]
fn gravity_metadata_and_subtick_sampling_have_no_authority_side_effects() {
    let (terrain, physics) = scene();
    let mut view = View::new(
        world::java::player::Motion {
            position: [8.5, 2., 8.5],
            velocity: [0.1, 0., 0.],
            ..Default::default()
        },
        [0.25; 2],
    );
    tick(&terrain, &physics, (7, 1), &mut view, 1, true).unwrap();
    assert_eq!(view.position[1], 2.);
    assert_eq!(view.velocity[1], 0.);
    assert!((view.sample(0.5)[0] - 8.55).abs() < 1e-12);
}

#[test]
fn native_static_partial_shape_landing_uses_clipped_height_and_ground_drag() {
    let (mut terrain, physics) = scene();
    terrain.set_block([8, 0, 8], BlockStateId(3));
    let mut view = View::new(
        world::java::player::Motion {
            position: [8.5, 0.75, 8.5],
            velocity: [0.1, -0.5, 0.],
            ..Default::default()
        },
        [0.25; 2],
    );
    // 26.3 ItemEntity.tick -> Entity.move clips against the native slab box, with no
    // entity step-up. Its getBlockPosBelowThatAffectsMyMovement uses .500001f Y offset.
    tick(&terrain, &physics, (7, 1), &mut view, 1, false).unwrap();
    assert_eq!(view.position, [8.6, 0.5, 8.5]);
    assert!(view.on_ground);
    assert_eq!(view.velocity, [0.1 * f64::from(0.98_f32 * 0.6_f32), 0., 0.]);
    assert_eq!(view.sample(0.5), [8.55, 0.625, 8.5]);
    assert!(view.active);
}
#[test]
fn extended_static_shapes_keep_explicit_transactional_fallback() {
    let (mut terrain, physics) = scene();
    terrain.set_block([8, 0, 8], BlockStateId(4));
    let mut view = View::new(
        world::java::player::Motion {
            position: [8.5, 2., 8.5],
            velocity: [0., -0.5, 0.],
            ..Default::default()
        },
        [0.25; 2],
    );
    let before = view.clone();
    assert!(matches!(
        tick(&terrain, &physics, (7, 1), &mut view, 1, false),
        Err(Error::Unsupported)
    ));
    assert_eq!(view.position, before.position);
    assert_eq!(view.velocity, before.velocity);
    assert_eq!(view.ticks, before.ticks);
}

#[test]
fn native_partial_shape_support_uses_footprint_xz_and_adjusted_y_for_drag() {
    let (mut terrain, physics) = scene();
    terrain.set_block([8, 0, 8], BlockStateId(3));
    terrain.set_block([8, -1, 8], BlockStateId(5));
    let mut view = View::new(
        world::java::player::Motion {
            position: [9.05, 0.75, 8.5],
            velocity: [0.01, -0.5, 0.],
            ..Default::default()
        },
        [0.25; 2],
    );
    // The center is beyond the slab, but the footprint still lands on it. Native getOnPos
    // takes X/Z from that supporting block and floors Y-.500001f, finding the ice below.
    tick(&terrain, &physics, (7, 1), &mut view, 1, false).unwrap();
    assert_eq!(view.position[1], 0.5);
    assert!(view.on_ground);
    assert_eq!(view.velocity[0], 0.01 * f64::from(0.98_f32 * 0.98_f32));
}
