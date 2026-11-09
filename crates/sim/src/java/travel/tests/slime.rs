use super::*;
use world::java::BlockStateId;

#[test]
fn slime_landing_bounces_and_sneaking_suppresses_it_without_blocking_motion() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let id = climbing::id(physics.registry().unwrap(), "minecraft:slime_block", &[]);
    for crouch in [false, true] {
        let mut terrain = climbing::scene([0, 0, 0]);
        terrain.set_block([8, 0, 8], BlockStateId(id));
        let mut motion = Motion {
            position: [8.5, 1.1, 8.5],
            velocity: [0., -0.8, 0.],
            ..Default::default()
        };
        let mut body = Body::default();
        step(
            &mut motion,
            &mut body,
            Input {
                crouch,
                ..Default::default()
            },
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert_eq!(motion.position[1], 1.);
        assert!(body.on_ground);
        if crouch {
            assert!(motion.velocity[1] < 0.);
        } else {
            assert!(motion.velocity[1] > 0.);
        }
    }
}
#[test]
fn walking_and_jumping_on_slime_are_not_unsupported_callbacks() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let id = climbing::id(physics.registry().unwrap(), "minecraft:slime_block", &[]);
    let mut terrain = climbing::scene([0, 0, 0]);
    for x in 7..=9 {
        for z in 7..=9 {
            terrain.set_block([x, 0, z], BlockStateId(id));
        }
    }
    for jump in [false, true] {
        let mut motion = Motion {
            position: [8.5, 1., 8.5],
            ..Default::default()
        };
        let mut body = Body {
            on_ground: true,
            ..Default::default()
        };
        step(
            &mut motion,
            &mut body,
            Input {
                forward: 1.,
                jump,
                ..Default::default()
            },
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert!(motion.position[2] > 8.5);
        if jump {
            assert!(motion.position[1] > 1.);
        }
    }
}

#[test]
fn native_restitution_and_step_callbacks_match_independent_runtime_vectors() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/fixtures/java-slime.json")).unwrap();
    let vector = |v: &serde_json::Value| std::array::from_fn::<_, 3, _>(|i| v[i].as_f64().unwrap());
    for row in vectors["landing"].as_array().unwrap() {
        let incoming = vector(&row["incoming"]);
        let actual = vector(&row["actual"]);
        let expected = vector(&row["result"]);
        let result = super::super::slime::restitute_y(
            incoming[1],
            actual[1],
            true,
            row["shift"].as_bool().unwrap(),
            row["restitution"].as_f64().unwrap() as f32,
            row["gravity"].as_f64().unwrap(),
            row["air_drag"].as_f64().unwrap() as f32,
        );
        assert_eq!(result.to_bits(), expected[1].to_bits(), "{row}");
    }
    for row in vectors["stepping"].as_array().unwrap() {
        let mut velocity = [0.2, row["vertical"].as_f64().unwrap(), -0.3];
        super::super::slime::step_on(&mut velocity, row["shift"].as_bool().unwrap());
        assert_eq!(
            velocity.map(f64::to_bits),
            vector(&row["result"]).map(f64::to_bits),
            "{row}"
        );
    }
}

#[test]
fn slime_support_uses_the_real_footprint_across_negative_chunk_edges_and_ties() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let slime = climbing::id(registry, "minecraft:slime_block", &[]);
    let stone = climbing::id(registry, "minecraft:stone", &[]);
    for shift in [[0, 0, 0], [-32, 0, -16]] {
        let mut terrain = climbing::scene([16 + shift[0], 0, shift[2]]);
        terrain.set_block([15 + shift[0], 0, shift[2]], BlockStateId(slime));
        let position = [16.1 + f64::from(shift[0]), 1.1, 0.5 + f64::from(shift[2])];
        let mut motion = Motion {
            position,
            velocity: [0., -0.8, 0.],
            ..Default::default()
        };
        let mut body = Body::default();
        step(
            &mut motion,
            &mut body,
            Input::default(),
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert_eq!(motion.position[1], 1.);
        assert!(motion.velocity[1] > 0., "support straddles chunk edge");
        terrain.set_block([16 + shift[0], 0, shift[2]], BlockStateId(stone));
        motion = Motion {
            position: [16. + f64::from(shift[0]), 1.1, 0.5 + f64::from(shift[2])],
            velocity: [0., -0.8, 0.],
            ..Default::default()
        };
        body = Body::default();
        step(
            &mut motion,
            &mut body,
            Input::default(),
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert_eq!(
            motion.velocity[1],
            -0.08 * f64::from(0.98_f32),
            "native equal-distance tie selects the greater X stone block"
        );
    }
}

#[test]
fn authoritative_suppression_tags_override_slime_and_unimplemented_callbacks_stay_explicit() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let id = climbing::id(registry, "minecraft:slime_block", &[]);
    let mut terrain = climbing::scene([0, 0, 0]);
    terrain.set_block([8, 0, 8], BlockStateId(id));
    let tag = [registry.block_id(id).unwrap() as i32];
    let mut p = terrain::parameters();
    p.suppresses_bounce = &tag;
    let mut motion = Motion {
        position: [8.5, 1.1, 8.5],
        velocity: [0., -0.8, 0.],
        ..Default::default()
    };
    let mut body = Body::default();
    step(
        &mut motion,
        &mut body,
        Input::default(),
        p,
        &terrain,
        &physics,
        (7, 1),
    )
    .unwrap();
    assert_eq!(motion.position[1], 1.);
    assert!(motion.velocity[1] < 0.);
    terrain.set_block(
        [8, 1, 8],
        BlockStateId(climbing::id(registry, "minecraft:cobweb", &[])),
    );
    let before = motion;
    assert!(matches!(
        step(
            &mut motion,
            &mut body,
            Input::default(),
            p,
            &terrain,
            &physics,
            (7, 1)
        ),
        Err(Error::Unsupported { .. })
    ));
    assert_eq!(
        motion.position, before.position,
        "unknown callbacks never commit a partial tick"
    );
}

#[test]
fn full_zero_input_vertical_travel_matches_controlled_native_living_entity_bounces() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let id = climbing::id(physics.registry().unwrap(), "minecraft:slime_block", &[]);
    let mut terrain = climbing::scene([0, 0, 0]);
    for x in 6..=10 {
        for z in 6..=10 {
            terrain.set_block([x, 0, z], BlockStateId(id));
        }
    }
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-slime-fall.json"
    ))
    .unwrap();
    let rows = data["ticks"].as_array().unwrap();
    let mut motion = Motion {
        position: [8.5, 7., 8.5],
        ..Default::default()
    };
    let mut body = Body::default();
    for (i, row) in rows.iter().enumerate() {
        if i != 0 {
            step(
                &mut motion,
                &mut body,
                Input::default(),
                terrain::parameters(),
                &terrain,
                &physics,
                (7, 1),
            )
            .unwrap();
        }
        assert!(
            (motion.position[1] - row["position"][1].as_f64().unwrap()).abs() < 1e-12,
            "tick {i}: position {:?} reference {row}",
            motion.position
        );
        assert!(
            (motion.velocity[1] - row["velocity"][1].as_f64().unwrap()).abs() < 1e-12,
            "tick {i}: velocity {:?} reference {row}",
            motion.velocity
        );
        assert_eq!(
            body.on_ground,
            row["on_ground"].as_bool().unwrap(),
            "tick {i}"
        );
    }
}

#[test]
fn ordinary_bed_restitution_uses_the_shared_landing_path() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let foot = climbing::id(
        registry,
        "minecraft:red_bed",
        &[("part", "foot"), ("facing", "south"), ("occupied", "false")],
    );
    let head = climbing::id(
        registry,
        "minecraft:red_bed",
        &[("part", "head"), ("facing", "south"), ("occupied", "false")],
    );
    let floor = physics.state(foot).unwrap();
    assert!(!floor.special_callbacks);
    assert!(floor.bounce_restitution > 0. && floor.bounce_restitution < 1.);
    let assets::registry::java::physics::Collision::Static(shape) = &floor.collision else {
        panic!("native bed requires its verified static shape")
    };
    let height = shape
        .iter()
        .map(|b| b.max[1])
        .max_by(f64::total_cmp)
        .unwrap();
    let mut terrain = climbing::scene([0, 0, 0]);
    terrain.set_block([8, 0, 8], BlockStateId(foot));
    terrain.set_block([8, 0, 9], BlockStateId(head));
    for crouch in [false, true] {
        let mut motion = Motion {
            position: [8.5, height + 0.1, 8.5],
            velocity: [0., -0.8, 0.],
            ..Default::default()
        };
        let mut body = Body::default();
        step(
            &mut motion,
            &mut body,
            Input {
                crouch,
                ..Default::default()
            },
            terrain::parameters(),
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert_eq!(motion.position[1], height);
        if crouch {
            assert!(motion.velocity[1] < 0.);
        } else {
            assert!(motion.velocity[1] > 0. && motion.velocity[1] < 0.8);
        }
    }
}
