use super::*;
use world::java::BlockStateId;

fn pool() -> (Terrain, Table) {
    let (mut t, p) = liquid::scene(8_f32 / 9., false);
    for x in 5..12 {
        for z in 5..12 {
            for y in 0..4 {
                t.set_block([x, y, z], BlockStateId(2));
            }
        }
    }
    (t, p)
}
fn input() -> Input {
    Input {
        forward: 1.,
        sprint: true,
        ..Default::default()
    }
}

#[test]
fn native_dimensions_and_source_classification_are_version_pinned() {
    let facts: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-swimming.json"
    ))
    .unwrap();
    let target: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    assert_eq!(facts["protocol"], target["protocol"]);
    for (name, body) in [
        ("STANDING", Body::default()),
        (
            "CROUCHING",
            Body {
                crouched: true,
                ..Default::default()
            },
        ),
        (
            "SWIMMING",
            Body {
                low_profile: true,
                ..Default::default()
            },
        ),
    ] {
        let d = &facts["poses"][name];
        let b = body.bounds([0.; 3], 1.);
        assert_eq!(
            b.max[0] - b.min[0],
            f64::from(d[0].as_f64().unwrap() as f32)
        );
        assert_eq!(b.max[1], f64::from(d[1].as_f64().unwrap() as f32));
        assert_eq!(
            body.eye_height(1.),
            f64::from(d[2].as_f64().unwrap() as f32)
        );
    }
    assert_eq!(facts["source_classification_states"], 13389);
}

#[test]
fn entering_swimming_uses_previous_sprint_and_eyes_then_changes_pose_after_travel() {
    let (t, p) = pool();
    let mut m = Motion {
        position: [8.5, 1., 8.5],
        ..Default::default()
    };
    let mut b = Body::default();
    step(
        &mut m,
        &mut b,
        input(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(!b.sprinting && b.eyes_in_water && !b.swimming && !b.low_profile);
    assert_eq!(
        m.position[2],
        8.5 + f64::from(0.98_f32) * f64::from(0.02_f32)
    );
    step(
        &mut m,
        &mut b,
        input(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(b.sprinting && !b.swimming && !b.low_profile);
    step(
        &mut m,
        &mut b,
        input(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(b.swimming && b.low_profile && !b.crouched);
    assert_eq!(b.eye_height(1.), f64::from(0.4_f32));
    step(
        &mut m,
        &mut b,
        Input::default(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(b.swimming && !b.sprinting);
    step(
        &mut m,
        &mut b,
        Input::default(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(!b.swimming && !b.low_profile);
}

#[test]
fn low_pose_is_retained_under_roof_after_swimming_stops_and_recovers_on_removal() {
    let (mut t, p) = pool();
    for x in 6..11 {
        for z in 6..11 {
            t.set_block([x, 1, z], BlockStateId(3));
        }
    }
    let mut m = Motion {
        position: [8.5, 0.8, 8.5],
        ..Default::default()
    };
    let mut b = Body {
        low_profile: true,
        swimming: true,
        sprinting: true,
        eyes_in_water: true,
        ..Default::default()
    };
    for _ in 0..3 {
        step(
            &mut m,
            &mut b,
            Input::default(),
            terrain::parameters(),
            &t,
            &p,
            (7, 1),
        )
        .unwrap();
    }
    assert!(!b.swimming && b.low_profile);
    assert!(m.position[1] + f64::from(0.6_f32) <= 1.5);
    for x in 6..11 {
        for z in 6..11 {
            t.set_block([x, 1, z], BlockStateId(2));
        }
    }
    step(
        &mut m,
        &mut b,
        Input::default(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(!b.low_profile && !b.crouched);
}

#[test]
fn swimming_look_steering_distinguishes_downward_upward_and_surface_escape() {
    let (mut t, p) = pool();
    let cells = super::super::liquid::Cells {
        terrain: &t,
        physics: &p,
    };
    let mut m = Motion {
        position: [8.5, 1., 8.5],
        pitch: 90.,
        ..Default::default()
    };
    super::super::swimming::steer(&mut m, false, &cells).unwrap();
    assert_eq!(m.velocity[1], -0.085);
    m.pitch = -90.;
    m.velocity = [0.; 3];
    super::super::swimming::steer(&mut m, false, &cells).unwrap();
    assert_eq!(m.velocity[1], 0.06);
    for x in 5..12 {
        for z in 5..12 {
            for y in 1..4 {
                t.set_block([x, y, z], BlockStateId(0));
            }
        }
    }
    let cells = super::super::liquid::Cells {
        terrain: &t,
        physics: &p,
    };
    m.position[1] = 0.5;
    m.velocity = [0.; 3];
    super::super::swimming::steer(&mut m, false, &cells).unwrap();
    assert_eq!(m.velocity[1], 0.);
    super::super::swimming::steer(&mut m, true, &cells).unwrap();
    assert_eq!(m.velocity[1], 0.06);
}

#[test]
fn eye_immersion_uses_actual_surface_and_missing_neighbors_do_not_commit_pose() {
    let (mut t, p) = pool();
    for x in 5..12 {
        for z in 5..12 {
            for y in 1..4 {
                t.set_block([x, y, z], BlockStateId(0));
            }
        }
    }
    let cells = super::super::liquid::Cells {
        terrain: &t,
        physics: &p,
    };
    assert!(cells.water_at_eye([8.5, 0., 8.5], 0.8).unwrap());
    assert!(!cells.water_at_eye([8.5, 0., 8.5], 0.9).unwrap());
    let mut m = Motion {
        position: [15.7, 0., 8.5],
        ..Default::default()
    };
    let before = m;
    let mut b = Body {
        sprinting: true,
        eyes_in_water: true,
        ..Default::default()
    };
    assert!(
        step(
            &mut m,
            &mut b,
            input(),
            terrain::parameters(),
            &t,
            &p,
            (7, 1)
        )
        .is_err()
    );
    assert_eq!(m.position, before.position);
    assert!(!b.swimming && b.eyes_in_water);
}

#[test]
fn underwater_crouch_can_start_swimming_but_dry_and_shallow_crouch_cannot_sprint() {
    for (submerged, may_fly, expected_sprint) in [
        (true, false, true),
        (false, false, false),
        (false, true, true),
    ] {
        let (t, p) = pool();
        let mut m = Motion {
            position: [8.5, 1., 8.5],
            ..Default::default()
        };
        let mut b = Body {
            eyes_in_water: submerged,
            ..Default::default()
        };
        let mut parameters = terrain::parameters();
        parameters.may_fly = may_fly;
        let controls = Input {
            crouch: submerged,
            ..input()
        };
        step(&mut m, &mut b, controls, parameters, &t, &p, (7, 1)).unwrap();
        assert_eq!(b.sprinting, expected_sprint);
        if submerged {
            step(&mut m, &mut b, controls, parameters, &t, &p, (7, 1)).unwrap();
            assert!(b.swimming && b.low_profile && !b.crouched);
            b.horizontal_collision = true;
            step(&mut m, &mut b, controls, parameters, &t, &p, (7, 1)).unwrap();
            assert!(b.sprinting && b.swimming);
        }
    }
}

#[test]
fn dry_low_pose_does_not_advertise_sprinting_under_a_crawl_ceiling() {
    let (mut t, p) = terrain::scene();
    for x in 5..12 {
        for z in 5..12 {
            t.set_block([x, 1, z], world::java::BlockStateId(1));
        }
    }
    let mut m = Motion {
        position: [8.5, 0., 8.5],
        ..Default::default()
    };
    let mut b = Body {
        low_profile: true,
        on_ground: true,
        ..Default::default()
    };
    step(
        &mut m,
        &mut b,
        input(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(!b.sprinting && b.low_profile && !b.swimming);
}

#[test]
fn first_swim_tick_checks_current_crouch_contacts_before_selecting_the_low_pose() {
    let (mut t, p) = liquid::scene(8_f32 / 9., false);
    for x in 6..11 {
        for z in 6..11 {
            for y in 13..16 {
                t.set_block([x, y, z], BlockStateId(2));
            }
        }
    }
    let mut m = Motion {
        position: [8.5, 14.3, 8.5],
        ..Default::default()
    };
    let mut b = Body {
        crouched: true,
        eyes_in_water: true,
        sprinting: true,
        ..Default::default()
    };
    step(
        &mut m,
        &mut b,
        input(),
        terrain::parameters(),
        &t,
        &p,
        (7, 1),
    )
    .unwrap();
    assert!(b.swimming && b.low_profile && !b.crouched);
    assert_eq!(m.position[1], 14.3);
}
