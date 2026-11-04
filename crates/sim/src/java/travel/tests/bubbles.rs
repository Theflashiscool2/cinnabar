use super::super::bubbles as behavior;
use super::climbing::{id, scene};
use super::*;
use world::java::BlockStateId;

#[test]
fn ordered_sweeps_and_impulses_match_independent_native_runtime_calls() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-block-contacts.json"
    ))
    .unwrap();
    let target: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    assert_eq!(f["game_version"], target["game_version"]);
    assert_eq!(f["protocol"], target["protocol"]);
    for c in f["visits"].as_array().unwrap() {
        let from = std::array::from_fn(|i| c["from"][i].as_f64().unwrap());
        let to = std::array::from_fn(|i| c["to"][i].as_f64().unwrap());
        let b = Bounds {
            min: std::array::from_fn(|i| c["bounds"][i].as_f64().unwrap()),
            max: std::array::from_fn(|i| c["bounds"][i + 3].as_f64().unwrap()),
        };
        let expected: Vec<_> = c["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    [
                        r[0].as_f64().unwrap() as i32,
                        r[1].as_f64().unwrap() as i32,
                        r[2].as_f64().unwrap() as i32,
                    ],
                    r[3].as_f64().unwrap() as i32,
                )
            })
            .collect();
        assert_eq!(
            collision::sweep::visits(from, to, b).unwrap(),
            expected,
            "from{from:?} to{to:?}"
        );
    }
    for c in f["impulses"].as_array().unwrap() {
        let mut y = c["input"].as_f64().unwrap();
        let mut fall = 4.;
        behavior::impulse(
            &mut y,
            &mut fall,
            c["drag"].as_bool().unwrap(),
            c["surface"].as_bool().unwrap(),
        );
        assert_eq!(y.to_bits(), c["velocity"][1].as_f64().unwrap().to_bits());
        assert_eq!(fall, c["fall"].as_f64().unwrap());
    }
}
#[test]
fn bubble_callbacks_follow_water_drag_then_authoritative_ceiling_and_removal() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let up = id(r, "minecraft:bubble_column", &[("drag", "false")]);
    let down = id(r, "minecraft:bubble_column", &[("drag", "true")]);
    let water = id(r, "minecraft:water", &[("level", "0")]);
    let stone = id(r, "minecraft:stone", &[]);
    for offset in [[8, 0, 8], [-16, -16, -16], [16, 16, 16]] {
        for (state, roof, expected) in [
            (up, water, 0.055),
            (down, water, -0.035),
            (up, 0, 0.095),
            (down, 0, -0.035),
            (up, stone, 0.055),
        ] {
            let mut t = scene(offset);
            t.set_block(offset, BlockStateId(state));
            t.set_block([offset[0], offset[1] + 1, offset[2]], BlockStateId(roof));
            let mut m = Motion {
                position: [
                    f64::from(offset[0]) + 0.5,
                    f64::from(offset[1]),
                    f64::from(offset[2]) + 0.5,
                ],
                ..Motion::default()
            };
            let mut b = Body {
                low_profile: roof == stone,
                ..Body::default()
            };
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
            assert!(
                (m.velocity[1] - expected).abs() < 1e-14,
                "{state} roof{roof}: {:?}",
                m.velocity
            );
            // Removal is read from the authoritative palette on the very next tick.
            t.set_block(offset, BlockStateId(water));
            m.velocity = [0.; 3];
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
            assert!((m.velocity[1] + 0.005).abs() < 1e-14);
        }
    }
}
#[test]
fn bubble_neighbor_query_failure_is_atomic_and_recovers_after_arrival() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let state = id(r, "minecraft:bubble_column", &[("drag", "false")]);
    let mut t = scene([15, 0, 8]);
    t.set_block([15, 0, 8], BlockStateId(state));
    let mut m = Motion {
        position: [15.9, 0., 8.5],
        ..Motion::default()
    };
    let mut b = Body::default();
    let column = (**t.column([1, 0]).unwrap()).clone();
    t.unload([1, 0]);
    let before = m;
    assert!(
        step(
            &mut m,
            &mut b,
            Input::default(),
            terrain::parameters(),
            &t,
            &p,
            (7, 1)
        )
        .is_err()
    );
    assert_eq!(m.position, before.position);
    assert_eq!(m.velocity, before.velocity);
    t.replace([1, 0], column);
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
    assert!(m.velocity[1] > 0.);
}

#[test]
fn opposing_columns_apply_once_per_block_in_native_axis_order_at_a_chunk_seam() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let up = id(r, "minecraft:bubble_column", &[("drag", "false")]);
    let down = id(r, "minecraft:bubble_column", &[("drag", "true")]);
    let water = id(r, "minecraft:water", &[("level", "0")]);
    for seam in [-16, 16] {
        let mut t = scene([seam, 0, 8]);
        for (x, state) in [(seam - 1, up), (seam, down)] {
            t.set_block([x, 0, 8], BlockStateId(state));
            t.set_block([x, 1, 8], BlockStateId(water));
        }
        for (from, dx, expected) in [
            (f64::from(seam) - 0.25, 0.25, 0.7 - 0.03),
            (f64::from(seam) + 0.25, -0.25, 0.7),
        ] {
            let mut m = Motion {
                position: [f64::from(seam), 0., 8.5],
                velocity: [0., 0.69, 0.],
                ..Motion::default()
            };
            let mut b = Body {
                fall_distance: 5.,
                ..Body::default()
            };
            behavior::apply(
                [from, 0., 8.5],
                [dx, 0., 0.],
                &mut m,
                &mut b,
                Body::default(),
                1.,
                &t,
                &p,
            )
            .unwrap();
            assert_eq!(m.velocity[1], expected);
            assert_eq!(b.fall_distance, 0.);
        }
    }
}

#[test]
fn magma_column_floor_keeps_damage_server_authoritative_without_blocking_travel() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let magma = id(r, "minecraft:magma_block", &[]);
    let column = id(r, "minecraft:bubble_column", &[("drag", "true")]);
    let mut t = scene([8, 0, 8]);
    t.set_block([8, -1, 8], BlockStateId(magma));
    t.set_block([8, 0, 8], BlockStateId(column));
    let mut m = Motion {
        position: [8.5, 0., 8.5],
        ..Motion::default()
    };
    let mut b = Body::default();
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
    assert_eq!(m.position, [8.5, 0., 8.5]);
    assert!((m.velocity[1] + 0.035).abs() < 1e-14);
}

#[test]
fn scaffold_ceilings_use_native_empty_context_instead_of_the_player_context() {
    let Some(p) = current_reference::native_physics() else {
        return;
    };
    let r = p.registry().unwrap();
    let up = id(r, "minecraft:bubble_column", &[("drag", "false")]);
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-bubble-ceilings.json"
    ))
    .unwrap();
    let target: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    assert_eq!(f["game_version"], target["game_version"]);
    assert_eq!(f["protocol"], target["protocol"]);
    let actual: std::collections::BTreeSet<_> = f["scaffold_ceilings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["state"].as_u64().unwrap() as u32)
        .collect();
    let expected: std::collections::BTreeSet<_> = (0..r.len() as u32)
        .filter(|id| {
            assets::registry::java::physics::climbing::kind(r, *id)
                == Some(assets::registry::java::physics::climbing::Kind::Scaffolding)
        })
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(actual, expected);
    for c in f["scaffold_ceilings"].as_array().unwrap() {
        assert_eq!(c["surface"], false);
        let mut t = scene([8, 0, 8]);
        t.set_block([8, 0, 8], BlockStateId(up));
        t.set_block([8, 1, 8], BlockStateId(c["state"].as_u64().unwrap() as u32));
        let mut m = Motion {
            position: [8.5, 0., 8.5],
            ..Motion::default()
        };
        let mut b = Body::default();
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
        assert!(
            (m.velocity[1] - 0.055).abs() < 1e-14,
            "{c}: {:?}",
            m.velocity
        );
    }
}
