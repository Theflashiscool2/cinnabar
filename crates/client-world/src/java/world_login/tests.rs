use super::*;
use crate::java::{player, tests_support};
use java_protocol::session::{Envelope, Event, facts, facts::Relative};
use std::sync::Arc;
use world::java::{entities, roster};

fn receive(state: &mut WorldState, event: Event) -> bool {
    state.admit(Envelope {
        generation: 7,
        event,
    })
}
#[test]
fn repeated_login_replaces_world_not_configuration_or_local_storage() {
    let mut state = tests_support::state();
    let registry =
        state.bootstrap.as_ref().unwrap().configuration.registries["minecraft:dimension_type"]
            .as_ptr();
    state.players.update(
        7,
        vec![roster::Update {
            uuid: [1; 16],
            profile: Some(Arc::new(roster::Profile {
                name: "Peer".into(),
                properties: Arc::from([]),
                default_skin: roster::DefaultSkin {
                    texture: "fixture:skin".into(),
                    slim: false,
                },
            })),
            ..Default::default()
        }],
    );
    state.players.update_team(
        7,
        roster::teams::Update {
            team: "old".into(),
            action: roster::teams::Action::Create,
            members: vec!["Peer".into()],
            parameters: Some(roster::teams::Parameters {
                display: world::NbtValue::String("Old".into()),
                prefix: world::NbtValue::String("".into()),
                suffix: world::NbtValue::String("".into()),
                visibility: roster::teams::Visibility::Never,
                collision: roster::teams::Collision::Always,
                color: None,
                options: 0,
            }),
        },
    );
    state
        .entities
        .spawn(
            (7, 1),
            entities::Spawn {
                network_id: 9,
                uuid: [1; 16],
                kind: "minecraft:pig".into(),
                motion: Default::default(),
                head_yaw: 0.,
                object_data: 0,
                visual: Default::default(),
                interpolation_interval: None,
            },
        )
        .unwrap();
    let stack = inventory::java::Stack {
        item: "minecraft:stone".into(),
        count: 5,
        added: Default::default(),
        removed: Default::default(),
    };
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 2,
                stack: Some(stack.clone()),
            },
        )
        .unwrap();
    state
        .inventory
        .admit(7, 1, inventory::java::Update::Selected(2))
        .unwrap();
    state.inventory.player.as_mut().unwrap().carried = Some(Some(stack));
    assert!(receive(
        &mut state,
        Event::Teleport {
            dimension_epoch: 1,
            sequence: 10,
            correction: facts::Teleport {
                id: 7,
                position: [1., 64., 2.],
                velocity: [8., 9., 10.],
                yaw: 30.,
                pitch: 45.,
                relative: 0,
            }
        }
    ));
    let mut world = state.bootstrap.as_ref().unwrap().world.clone();
    world.entity_id = 84;
    world.spawn.hashed_seed = 99;
    assert!(receive(&mut state, Event::WorldLogin(Box::new(world))));
    assert_eq!(state.terrain.dimension().unwrap().epoch, 2);
    assert_eq!(state.entities.iter().count(), 0);
    assert!(state.players.get(&[1; 16]).is_some());
    assert!(state.players.team_parameters("Peer").is_none());
    assert_eq!(
        state.bootstrap.as_ref().unwrap().configuration.registries["minecraft:dimension_type"]
            .as_ptr(),
        registry
    );
    assert_eq!(state.bootstrap.as_ref().unwrap().world.entity_id, 84);
    assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 5);
    assert_eq!(state.inventory.selected, Some(2));
    assert!(state.inventory.player.as_ref().unwrap().carried.is_none());
    assert!(state.player_motion.is_none() && state.teleport.is_none());
    assert!(state.correction_command().is_none());
    assert!(state.inventory.accepts(7, 2));
    assert!(!state.inventory.accepts(7, 1));
    assert!(receive(
        &mut state,
        Event::Teleport {
            dimension_epoch: 2,
            sequence: 11,
            correction: facts::Teleport {
                id: 7,
                position: [2., 3., 4.],
                velocity: [5., 6., 7.],
                yaw: 15.,
                pitch: 5.,
                relative: [
                    Relative::X,
                    Relative::Y,
                    Relative::Z,
                    Relative::Yaw,
                    Relative::Pitch,
                    Relative::VelocityX,
                    Relative::VelocityY,
                    Relative::VelocityZ
                ]
                .into_iter()
                .fold(0, |mask, f| mask | f.mask()),
            }
        }
    ));
    let motion = state.player_motion.unwrap();
    assert_eq!(motion.position, [3., 67., 6.]);
    assert_eq!(motion.velocity, [5., 6., 7.]);
    assert_eq!((motion.yaw, motion.pitch), (45., 5.));
    assert!(
        !player::admit(
            &mut state,
            1,
            12,
            facts::Teleport {
                id: 7,
                position: [0.; 3],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: 0
            }
        )
        .unwrap()
    );
    assert_eq!(state.player_motion.unwrap().position, motion.position);
}
#[test]
fn replacement_requires_initial_bootstrap_valid_identity_and_current_generation() {
    let mut state = tests_support::state();
    let world = state.bootstrap.as_ref().unwrap().world.clone();
    assert!(!state.admit(Envelope {
        generation: 6,
        event: Event::WorldLogin(Box::new(world.clone()))
    }));
    assert_eq!(state.terrain.dimension().unwrap().epoch, 1);
    let mut invalid = world.clone();
    invalid.spawn.dimension_type = 100;
    assert!(!receive(&mut state, Event::WorldLogin(Box::new(invalid))));
    assert_eq!(state.terrain.dimension().unwrap().epoch, 1);
    assert_eq!(
        state.bootstrap.as_ref().unwrap().world.spawn.dimension_type,
        0
    );
    state.begin(8);
    assert!(!state.admit(Envelope {
        generation: 8,
        event: Event::WorldLogin(Box::new(world))
    }));
}
