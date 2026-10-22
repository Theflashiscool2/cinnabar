use super::*;

fn spawn(id: i32, uuid: u8) -> Spawn {
    Spawn {
        network_id: id,
        uuid: [uuid; 16],
        kind: "fixture:entity".into(),
        motion: Motion {
            position: [0.12345, 64., -0.10003],
            ..Default::default()
        },
        head_yaw: 0.,
        object_data: 0,
    }
}

#[test]
fn native_delta_rounding_matches_independent_vanilla_vectors() {
    // Evaluated with the manifest-pinned server VecDeltaCodec, not this implementation.
    for (base, bits) in [
        (
            -1. / 8192.,
            [0x3f30000000000000, 0xbf30000000000000, 0xbf20000000000000],
        ),
        (
            1. / 8192.,
            [0x3f40000000000000, 0x0000000000000000, 0x3f20000000000000],
        ),
        (
            -0.10003,
            [0xbfb9900000000000, 0xbfb9b00000000000, 0xbfb99b90ea9e6eeb],
        ),
        (
            0.12345,
            [0x3fbfb00000000000, 0x3fbf900000000000, 0x3fbf9a6b50b0f27c],
        ),
        (
            -30000000.125,
            [0xc17c9c3801ff0000, 0xc17c9c3802010000, 0xc17c9c3802000000],
        ),
        (
            30000000.125,
            [0x417c9c3802010000, 0x417c9c3801ff0000, 0x417c9c3802000000],
        ),
    ] {
        assert_eq!(decode_delta([base; 3], [1, -1, 0]).map(f64::to_bits), bits);
    }
    for (value, expected) in [
        (-0.5, 0),
        (0.5, 1),
        (-4503599627370495.5, -4503599627370495),
        (4503599627370495.5, 4503599627370496),
        (f64::MAX, i64::MAX),
        (-f64::MAX, i64::MIN),
    ] {
        assert_eq!(java_round(value), expected);
    }
}

#[test]
fn reused_ids_get_new_lifetimes_and_retired_scopes_cannot_mutate_them() {
    let mut entities = Entities::default();
    entities.begin(7, 1);
    assert!(entities.spawn((7, 1), spawn(42, 1)).unwrap());
    let first = entities.get(42).unwrap().identity;
    entities
        .update((7, 1), 42, |e| {
            e.path = Arc::from([PositionStep {
                position: [1.; 3],
                tick_offset: 2,
            }]);
            Ok(())
        })
        .unwrap();
    assert!(entities.remove((7, 1), &[42]).unwrap());
    entities.spawn((7, 1), spawn(42, 2)).unwrap();
    let second = entities.get(42).unwrap();
    assert_ne!(first, second.identity);
    assert_eq!(second.uuid, [2; 16]);
    assert!(second.path.is_empty());
    assert_eq!(second.teleport_revision, 1);
    entities.begin(7, 2);
    assert_eq!(entities.iter().count(), 0);
    entities.spawn((7, 2), spawn(42, 3)).unwrap();
    assert!(
        !entities
            .update((7, 1), 42, |_| panic!("retired update was evaluated"))
            .unwrap()
    );
    assert!(!entities.remove((7, 1), &[42]).unwrap());
    assert!(!entities.spawn((6, 2), spawn(42, 4)).unwrap());
    assert_eq!(entities.get(42).unwrap().uuid, [3; 16]);
}

#[test]
fn invalid_updates_are_atomic_and_teleports_replace_interpolation() {
    let mut entities = Entities::default();
    entities.begin(7, 1);
    assert!(
        !entities
            .update((7, 1), 99, |_| panic!("unknown ID created state"))
            .unwrap()
    );
    assert_eq!(entities.unknown_updates, 1);
    entities.spawn((7, 1), spawn(42, 1)).unwrap();
    let revision = entities.revision();
    let old = entities.get(42).unwrap().motion;
    assert!(
        entities
            .update((7, 1), 42, |e| {
                e.motion.position[0] = f64::NAN;
                Ok(())
            })
            .is_err()
    );
    assert!(
        entities
            .update((7, 1), 42, |e| {
                e.identity.incarnation += 1;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(entities.revision(), revision);
    assert_eq!(entities.get(42).unwrap().motion, old);
    entities
        .update((7, 1), 42, |e| {
            e.path = Arc::from([PositionStep {
                position: [1.; 3],
                tick_offset: 2,
            }]);
            e.teleport(
                Motion {
                    position: [3., 70., 4.],
                    yaw: 90.,
                    ..Default::default()
                },
                Relative::default(),
                true,
            )
        })
        .unwrap();
    let e = entities.get(42).unwrap();
    assert_eq!(e.delta_base, [3., 70., 4.]);
    assert!(e.path.is_empty());
    assert_eq!(e.teleport_revision, 2);
    assert!(e.on_ground);
    assert_eq!(e.motion.yaw, 90.);
}

#[test]
fn passenger_references_detach_on_replacement_without_creating_a_second_local_player() {
    let mut entities = Entities::default();
    entities.begin(7, 1);
    for id in [99, 100, 101] {
        entities.spawn((7, 1), spawn(id, id as u8)).unwrap();
    }
    entities
        .set_passengers((7, 1), 99, &[42, 100, 100, 999], 42)
        .unwrap();
    assert_eq!(entities.passengers(99).unwrap().members.len(), 2);
    assert_eq!(
        entities.local_vehicle(),
        Some(entities.get(99).unwrap().identity)
    );
    assert!(entities.get(42).is_none());
    assert_eq!(entities.unknown_updates, 1);
    // A ride cannot make an ancestor its passenger.
    entities.set_passengers((7, 1), 100, &[99], 42).unwrap();
    assert!(entities.passengers(100).unwrap().members.is_empty());
    entities.set_passengers((7, 1), 101, &[42], 42).unwrap();
    assert_eq!(entities.passengers(99).unwrap().members.len(), 1);
    assert_eq!(
        entities.local_vehicle(),
        Some(entities.get(101).unwrap().identity)
    );
    entities.remove((7, 1), &[101]).unwrap();
    assert!(entities.local_vehicle().is_none());
    entities.spawn((7, 1), spawn(100, 3)).unwrap();
    assert!(entities.passengers(99).unwrap().members.is_empty());
    assert!(!entities.set_passengers((7, 0), 99, &[42], 42).unwrap());
    assert!(entities.local_vehicle().is_none());
}
