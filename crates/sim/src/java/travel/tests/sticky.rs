use super::super::sticky;
use super::*;
use world::java::BlockStateId;

#[test]
fn honey_boundaries_and_web_multipliers_match_actual_native_callbacks() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/fixtures/java-sticky.json")).unwrap();
    let vector = |v: &serde_json::Value| std::array::from_fn::<_, 3, _>(|i| v[i].as_f64().unwrap());
    for row in vectors["honey"].as_array().unwrap() {
        let mut motion = Motion {
            position: vector(&row["position"]),
            velocity: [0.2, row["vertical"].as_f64().unwrap(), -0.3],
            ..Default::default()
        };
        let mut body = Body {
            on_ground: row["grounded"].as_bool().unwrap(),
            fall_distance: 9.,
            ..Default::default()
        };
        sticky::honey(&mut motion, &mut body, [0; 3], 1.);
        assert_eq!(
            motion.velocity.map(f64::to_bits),
            vector(&row["result"]).map(f64::to_bits),
            "{row}"
        );
        assert_eq!(body.fall_distance, row["fall"].as_f64().unwrap(), "{row}");
    }
    for row in vectors["web"].as_array().unwrap() {
        let mut body = Body {
            fall_distance: 9.,
            ..Default::default()
        };
        sticky::web(&mut body, row["weaving"].as_bool().unwrap());
        assert_eq!(
            body.stuck_speed_multiplier.map(f64::to_bits),
            vector(&row["multiplier"]).map(f64::to_bits)
        );
        assert_eq!(body.fall_distance, row["fall"].as_f64().unwrap());
    }
}

#[test]
fn entity_bounce_matches_native_ceiling_floor_and_wall_restitution() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/fixtures/java-sticky.json")).unwrap();
    for row in vectors["bounce"].as_array().unwrap() {
        let incoming = row["vertical"].as_f64().unwrap();
        let shift = row["shift"].as_bool().unwrap();
        let bounce = row["bounce"].as_f64().unwrap();
        let y = super::super::slime::restitute_y(
            incoming,
            incoming * 0.25,
            row["below"].as_bool().unwrap(),
            shift,
            bounce,
            0.08,
            0.98,
        );
        assert_eq!(
            y.to_bits(),
            row["result"][1].as_f64().unwrap().to_bits(),
            "{row}"
        );
    }
}

#[test]
fn cobweb_contact_slows_the_next_move_and_is_consumed_after_exit() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let web = climbing::id(registry, "minecraft:cobweb", &[]);
    let air = climbing::id(registry, "minecraft:air", &[]);
    for weaving in [false, true] {
        let mut terrain = climbing::scene([0; 3]);
        terrain.set_block([8, 2, 8], BlockStateId(web));
        let mut motion = Motion {
            position: [8.5, 2.5, 8.5],
            velocity: [0.4, -0.2, 0.],
            ..Default::default()
        };
        let mut body = Body {
            fall_distance: 5.,
            ..Default::default()
        };
        let mut p = terrain::parameters();
        p.weaving = weaving;
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
        assert_eq!(
            motion.position,
            [8.9, 2.3, 8.5],
            "entry movement is not retroactively slowed"
        );
        assert_eq!(body.fall_distance, 0.);
        terrain.set_block([8, 2, 8], BlockStateId(air));
        let before = motion;
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
        let multiplier = if weaving {
            [0.5, 0.25, 0.5]
        } else {
            [0.25, f64::from(0.05_f32), 0.25]
        };
        for i in 0..3 {
            assert_eq!(
                motion.position[i],
                before.position[i] + before.velocity[i] * multiplier[i]
            );
        }
        assert_eq!(motion.velocity[0], 0.);
        assert_eq!(body.stuck_speed_multiplier, [0.; 3]);
        let before = motion;
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
        assert_eq!(motion.position[1], before.position[1] + before.velocity[1]);
    }
}

#[test]
fn honey_contact_slides_across_positive_and_negative_chunk_coordinates() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let honey = climbing::id(physics.registry().unwrap(), "minecraft:honey_block", &[]);
    for shift in [0, -16] {
        let mut terrain = climbing::scene([shift, 0, shift]);
        terrain.set_block([8 + shift, 0, 8 + shift], BlockStateId(honey));
        let mut motion = Motion {
            position: [9.24 + f64::from(shift), 0.5, 8.5 + f64::from(shift)],
            velocity: [0., -0.8, 0.],
            ..Default::default()
        };
        let mut body = Body {
            fall_distance: 9.,
            ..Default::default()
        };
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
        assert_eq!(motion.velocity[1], (-0.05 - 0.08) * f64::from(0.98_f32));
        assert_eq!(body.fall_distance, 0.);
        assert!(!body.on_ground);
    }
}

#[test]
fn bounciness_on_an_ordinary_floor_and_sneak_suppression() {
    let (terrain, physics) = terrain::scene();
    for crouch in [false, true] {
        let mut motion = Motion {
            position: [8.5, 0.2, 8.5],
            velocity: [0.4, -0.8, 0.],
            ..Default::default()
        };
        let mut body = Body::default();
        let mut p = terrain::parameters();
        p.bounciness = 0.3;
        step(
            &mut motion,
            &mut body,
            Input {
                crouch,
                ..Default::default()
            },
            p,
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert!(body.on_ground);
        if crouch {
            assert!(motion.velocity[1] < 0.);
        } else {
            assert!(motion.velocity[1] > 0.);
        }
    }
}

#[test]
fn bounciness_reverses_wall_velocity_and_sneaking_suppresses_it() {
    let (mut terrain, physics) = terrain::scene();
    for y in 2..=3 {
        terrain.set_block([9, y, 8], BlockStateId(1));
    }
    for crouch in [false, true] {
        let mut motion = Motion {
            position: [8.5, 2., 8.5],
            velocity: [0.4, 0., 0.],
            ..Default::default()
        };
        let mut body = Body::default();
        let mut p = terrain::parameters();
        p.bounciness = 0.3;
        step(
            &mut motion,
            &mut body,
            Input {
                crouch,
                ..Default::default()
            },
            p,
            &terrain,
            &physics,
            (7, 1),
        )
        .unwrap();
        assert!(body.horizontal_collision);
        assert_eq!(
            motion.velocity[0],
            if crouch {
                0.
            } else {
                -0.4 * 0.3 * f64::from(0.91_f32)
            }
        );
    }
}
