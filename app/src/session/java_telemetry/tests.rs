use super::*;
use java_protocol::session::facts::{Relative, Teleport};
fn fixture() -> (tempfile::TempDir, Session, client_world::java::WorldState) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("java-telemetry.enabled"), b"1").unwrap();
    let session =
        Session::prepare(std::future::pending()).with_report_directory(directory.path().to_owned());
    let mut world = client_world::java::WorldState::default();
    world.begin(7);
    world.terrain.begin(world::java::Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:telemetry".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    world.entities.begin(7, 1);
    use java_protocol::session::facts;
    world.bootstrap = Some(Box::new(facts::Bootstrap {
        generation: 7,
        profile: java_protocol::auth::Profile {
            id: Default::default(),
            name: "Fixture".into(),
        },
        server_session: Default::default(),
        configuration: Default::default(),
        world: facts::World {
            entity_id: -1,
            hardcore: false,
            worlds: vec!["fixture:telemetry".into()],
            max_players: 2,
            view_distance: 2,
            simulation_distance: 2,
            reduced_debug: false,
            respawn_screen: true,
            limited_crafting: false,
            online_mode: false,
            secure_chat: false,
            spawn: facts::Spawn {
                dimension_type: 0,
                world: "fixture:telemetry".into(),
                hashed_seed: 0,
                game_mode: 0,
                previous_game_mode: -1,
                debug: false,
                flat: true,
                last_death: None,
                portal_cooldown: 0,
                sea_level: 63,
            },
        },
    }));
    (directory, session, world)
}
fn envelope(event: Event) -> Envelope {
    Envelope {
        generation: 7,
        event,
    }
}
fn teleport(sequence: u64, epoch: u64, position: [f64; 3], relative: u32) -> Envelope {
    envelope(Event::Teleport {
        dimension_epoch: epoch,
        sequence,
        correction: Teleport {
            id: sequence as i32,
            position,
            velocity: [0.; 3],
            yaw: 0.,
            pitch: 0.,
            relative,
        },
    })
}
#[test]
fn observed_correction_uses_resolved_relative_position_and_preserves_authority() {
    let (_dir, session, mut world) = fixture();
    let (_other, _, mut direct) = fixture();
    let first = teleport(1, 1, [10., 64., 10.], 0);
    assert!(admit(&session, &mut world, first.clone()));
    assert!(direct.admit(first));
    let relative = teleport(2, 1, [3., 64., 4.], Relative::X.mask() | Relative::Z.mask());
    assert!(admit(&session, &mut world, relative.clone()));
    assert!(direct.admit(relative));
    assert_eq!(
        world.player_motion.unwrap().position,
        direct.player_motion.unwrap().position
    );
    assert_eq!(
        world.correction_command().unwrap().0,
        direct.correction_command().unwrap().0
    );
    let t = session.telemetry().unwrap().snapshot();
    assert_eq!(t["counts"]["initial_server_positions"]["count"], 1);
    assert_eq!(t["counts"]["server_position_corrections"]["count"], 1);
    assert_eq!(t["correction_distance_m"]["max"], 5.);
    let before = world.player_motion.unwrap();
    assert!(!admit(&session, &mut world, teleport(3, 99, [1.; 3], 0)));
    assert_eq!(world.player_motion.unwrap().position, before.position);
    assert_eq!(
        session.telemetry().unwrap().snapshot()["counts"]["teleport_admission_refused"]["count"],
        1
    );
}
#[test]
fn entity_collision_and_missing_despawn_are_observed_without_weakening_lifetimes() {
    let (_dir, session, mut world) = fixture();
    let spawn = |lifetime| {
        envelope(Event::Entities {
            dimension_epoch: 1,
            packet: Packet::Spawn(java_protocol::session::entities::Spawn {
                lifetime,
                id: 42,
                uuid: [1; 16],
                kind: "minecraft:snowball",
                position: [0., 64., 0.],
                velocity: [0.; 3],
                pitch: 0.,
                yaw: 0.,
                head_yaw: 0.,
                data: 0,
            }),
        })
    };
    assert!(admit(&session, &mut world, spawn(1)));
    let old = world.entities.get(42).unwrap().identity;
    assert!(admit(&session, &mut world, spawn(2)));
    assert_ne!(world.entities.get(42).unwrap().identity, old);
    admit(
        &session,
        &mut world,
        envelope(Event::Entities {
            dimension_epoch: 1,
            packet: Packet::Remove(vec![999]),
        }),
    );
    let t = session.telemetry().unwrap().snapshot();
    assert_eq!(t["counts"]["spawn_over_existing_entity_id"]["count"], 1);
    assert_eq!(t["counts"]["despawns_without_current_identity"]["count"], 1);
    assert_eq!(world.entities.get(42).unwrap().transport_lifetime, 2);
}
