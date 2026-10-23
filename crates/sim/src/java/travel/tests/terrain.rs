use super::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use world::java::{BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section};

fn scene() -> (Terrain, Table) {
    // Minimal independent report: air, full cube, water and an upper slab.
    let mut bytes = b"CBJPHYS1".to_vec();
    bytes.extend(4u32.to_be_bytes());
    for id in 0..4u32 {
        bytes.extend(id.to_be_bytes());
        bytes.push(0);
        for n in [0.6_f32, 1., 1., 0.] {
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
        bytes.extend(u32::from(id == 1 || id == 3).to_be_bytes());
        if id == 1 || id == 3 {
            for n in [0_f64, if id == 3 { 0.5 } else { 0. }, 0., 1., 1., 1.] {
                bytes.extend(n.to_be_bytes());
            }
        }
    }
    let table = Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 4).unwrap();
    let mut terrain = Terrain::default();
    terrain.begin(Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:floor".into(),
        min_y: -16,
        height: 32,
        has_skylight: true,
    });
    assert!(
        terrain.replace(
            [0, 0],
            Column::new(
                [1, 0]
                    .into_iter()
                    .map(|id| Arc::new(Section {
                        blocks: Palette::uniform(BlockStateId(id), 4096),
                        biomes: Palette::uniform(BiomeId(0), 64),
                    }))
                    .collect(),
                Light {
                    sky: vec![None; 4],
                    block: vec![None; 4]
                },
                vec![],
                vec![]
            )
        )
    );
    (terrain, table)
}
fn parameters() -> Parameters {
    Parameters {
        speed: 0.1,
        sprint_speed: 0.13,
        friction_modifier: 1.,
        air_drag_modifier: 1.,
        gravity: 0.08,
        jump_strength: 0.42,
        step_height: 0.6,
        scale: 1.,
        sneaking_speed: 0.3,
        movement_efficiency: 0.,
    }
}

#[test]
fn falling_lands_jumping_leaves_ground_and_walking_cannot_cross_a_wall() {
    let (mut terrain, table) = scene();
    let mut motion = Motion {
        position: [8.5, 2., 8.5],
        ..Default::default()
    };
    let mut body = Body::default();
    for _ in 0..20 {
        step(
            &mut motion,
            &mut body,
            Input::default(),
            parameters(),
            &terrain,
            &table,
            (7, 1),
        )
        .unwrap();
    }
    assert_eq!(motion.position[1], 0.);
    assert!(body.on_ground);
    step(
        &mut motion,
        &mut body,
        Input {
            jump: true,
            ..Default::default()
        },
        parameters(),
        &terrain,
        &table,
        (7, 1),
    )
    .unwrap();
    assert_eq!(motion.position[1], f64::from(0.42_f32));
    assert!(!body.on_ground);
    for _ in 0..30 {
        step(
            &mut motion,
            &mut body,
            Input::default(),
            parameters(),
            &terrain,
            &table,
            (7, 1),
        )
        .unwrap();
    }
    assert_eq!(motion.position[1], 0.);
    assert!(body.on_ground);
    for y in 0..3 {
        for x in 7..10 {
            terrain.set_block([x, y, 10], BlockStateId(1));
        }
    }
    for _ in 0..40 {
        step(
            &mut motion,
            &mut body,
            Input {
                forward: 1.,
                ..Default::default()
            },
            parameters(),
            &terrain,
            &table,
            (7, 1),
        )
        .unwrap();
    }
    assert_eq!(motion.position, [8.5, 0., 10. - f64::from(0.3_f32)]);
    assert!(body.horizontal_collision && body.on_ground);
    assert_eq!(motion.velocity[2], 0.);
}

#[test]
fn missing_or_unsupported_terrain_does_not_partially_advance_authority() {
    let (mut terrain, table) = scene();
    let before = Motion {
        position: [8.5, 0., 8.5],
        velocity: [0.1, 0., 0.],
        ..Default::default()
    };
    let mut motion = before;
    let mut body = Body::default();
    assert!(matches!(
        step(
            &mut motion,
            &mut body,
            Input::default(),
            parameters(),
            &terrain,
            &table,
            (7, 2)
        ),
        Err(Error::Query(QueryError::Retired))
    ));
    assert_eq!(motion, before);
    terrain.set_block([8, 0, 8], BlockStateId(2));
    assert!(matches!(
        step(
            &mut motion,
            &mut body,
            Input::default(),
            parameters(),
            &terrain,
            &table,
            (7, 1)
        ),
        Err(Error::Unsupported { .. })
    ));
    assert_eq!(motion, before);
    assert!(!body.on_ground);
    terrain.unload([0, 0]);
    assert!(matches!(
        step(
            &mut motion,
            &mut body,
            Input::default(),
            parameters(),
            &terrain,
            &table,
            (7, 1)
        ),
        Err(Error::Query(QueryError::Missing(_)))
    ));
    assert_eq!(motion, before);
}

#[test]
fn forced_crouch_cannot_keep_sprint_acceleration() {
    let (mut terrain, table) = scene();
    terrain.set_block([8, 1, 8], BlockStateId(3));
    let start = Motion {
        position: [8.5, 0., 8.5],
        ..Default::default()
    };
    let mut forced = start;
    let mut manual = start;
    let grounded = Body {
        on_ground: true,
        ..Default::default()
    };
    let mut forced_body = grounded;
    let mut manual_body = grounded;
    step(
        &mut forced,
        &mut forced_body,
        Input {
            forward: 1.,
            sprint: true,
            ..Default::default()
        },
        parameters(),
        &terrain,
        &table,
        (7, 1),
    )
    .unwrap();
    step(
        &mut manual,
        &mut manual_body,
        Input {
            forward: 1.,
            crouch: true,
            ..Default::default()
        },
        parameters(),
        &terrain,
        &table,
        (7, 1),
    )
    .unwrap();
    assert!(forced_body.crouched && !forced_body.sprinting);
    assert_eq!(forced.position, manual.position);
    assert_eq!(forced.velocity, manual.velocity);
}
