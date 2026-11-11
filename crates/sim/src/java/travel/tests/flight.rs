use super::*;
use world::java::BlockStateId;

#[test]
fn server_flight_air_travel_matches_the_official_player_wrapper() {
    let (terrain, physics) = terrain::scene();
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../../tests/fixtures/java-flight.json")).unwrap();
    for row in data["air"].as_array().unwrap() {
        let speed = row["speed"].as_f64().unwrap() as f32;
        let mut p = terrain::parameters();
        p.flying_speed = Some(speed);
        p.may_fly = true;
        let input = Input {
            forward: 1.,
            jump: row["jump"].as_bool().unwrap(),
            crouch: row["shift"].as_bool().unwrap(),
            sprint: row["sprint"].as_bool().unwrap(),
            ..Default::default()
        };
        let mut motion = Motion {
            position: [8.5, 8., 8.5],
            velocity: [0.125, row["initial_y"].as_f64().unwrap(), -0.25],
            ..Default::default()
        };
        let mut body = Body::default();
        step(&mut motion, &mut body, input, p, &terrain, &physics, (7, 1)).unwrap();
        let expected = std::array::from_fn::<_, 3, _>(|i| row["velocity"][i].as_f64().unwrap());
        assert_eq!(
            motion.velocity.map(f64::to_bits),
            expected.map(f64::to_bits),
            "{row}"
        );
        let moved = std::array::from_fn::<_, 3, _>(|i| row["move"][i].as_f64().unwrap());
        let expected_position = std::array::from_fn(|i| [8.5, 8., 8.5][i] + moved[i]);
        assert_eq!(motion.position, expected_position, "{row}");
        assert!(!body.crouched);
        assert!(!body.swimming);
    }
}

#[test]
fn flying_contacts_preserve_wrapper_vertical_damping_and_real_ground_flags() {
    for ceiling in [false, true] {
        let (mut terrain, physics) = terrain::scene();
        if ceiling {
            terrain.set_block([8, 3, 8], BlockStateId(1));
        }
        let mut p = terrain::parameters();
        p.flying_speed = Some(0.05);
        let mut motion = Motion {
            position: [8.5, if ceiling { 1.1 } else { 0.1 }, 8.5],
            velocity: [0., if ceiling { 0.2 } else { -0.2 }, 0.],
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
        assert_eq!(body.on_ground, !ceiling);
        assert_eq!(
            motion.velocity[1],
            if ceiling { 0.12 } else { -0.12 },
            "Player.travel restores saved Y even on collision"
        );
        assert_eq!(
            motion.position[1],
            if ceiling { 3. - f64::from(1.8_f32) } else { 0. }
        );
    }
}

#[test]
fn spectator_passes_through_real_blocks_and_does_not_consume_web_contact_state() {
    let (mut terrain, physics) = terrain::scene();
    terrain.set_block([9, 2, 8], BlockStateId(1));
    terrain.set_block([9, 3, 8], BlockStateId(1));
    let mut p = terrain::parameters();
    p.flying_speed = Some(0.05);
    p.spectator = true;
    let mut motion = Motion {
        position: [8.5, 2., 8.5],
        velocity: [1., 0., 0.],
        ..Default::default()
    };
    let mut body = Body {
        on_ground: true,
        horizontal_collision: true,
        stuck_speed_multiplier: [0.25, 0.05, 0.25],
        ..Default::default()
    };
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
    assert_eq!(motion.position, [9.5, 2., 8.5]);
    assert!(!body.on_ground);
    assert!(!body.horizontal_collision);
    assert_eq!(body.stuck_speed_multiplier, [0.25, 0.05, 0.25]);
}

#[test]
fn active_flight_uses_air_drag_in_water_and_skips_bubble_impulses() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let mut terrain = climbing::scene([0; 3]);
    let bubble = climbing::id(registry, "minecraft:bubble_column", &[("drag", "false")]);
    let water = climbing::id(registry, "minecraft:water", &[("level", "0")]);
    for y in 0..4 {
        terrain.set_block([8, y, 8], BlockStateId(if y == 0 { bubble } else { water }));
    }
    let mut p = terrain::parameters();
    p.flying_speed = Some(0.05);
    let mut motion = Motion {
        position: [8.5, 0., 8.5],
        velocity: [0.1, 0., 0.],
        ..Default::default()
    };
    let mut body = Body {
        swimming: true,
        ..Default::default()
    };
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
    assert_eq!(motion.velocity[1], 0.);
    assert_eq!(motion.velocity[0], 0.1 * f64::from(0.91_f32));
    assert!(!body.swimming);
    assert!(body.water_height > 0.);
}

#[test]
fn held_shift_selects_crouching_on_the_native_auto_land_tick() {
    let (terrain, physics) = terrain::scene();
    let mut p = terrain::parameters();
    p.flying_speed = Some(0.05);
    let mut motion = Motion {
        position: [8.5, 0.1, 8.5],
        velocity: [0., -0.2, 0.],
        ..Default::default()
    };
    let mut body = Body::default();
    step(
        &mut motion,
        &mut body,
        Input {
            crouch: true,
            ..Default::default()
        },
        p,
        &terrain,
        &physics,
        (7, 1),
    )
    .unwrap();
    assert!(body.on_ground);
    assert!(
        body.crouched,
        "LocalPlayer disables flight before Player.updatePlayerPose"
    );
    assert!(!body.low_profile);
}
