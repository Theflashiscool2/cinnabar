use super::*;
use java_protocol::session::{Envelope, Event, Phase};

#[test]
fn live_entities_keep_their_removed_player_info_without_adopting_a_reused_profile() {
    use java_protocol::session::{entities, hud::GameMode as WireMode};
    use world::java::hud::GameMode;
    let mut state = super::super::tests_support::state();
    let update = |profile: bool, mode| {
        Event::Roster(wire::Packet::Update(vec![wire::Update {
            uuid: [1; 16],
            profile: profile.then(|| wire::Profile {
                name: "Peer".into(),
                properties: vec![],
            }),
            mode: Some(mode),
            ..Default::default()
        }]))
    };
    let spawn = || Event::Entities {
        dimension_epoch: 1,
        packet: entities::Packet::Spawn(entities::Spawn {
            lifetime: 9,
            id: 99,
            uuid: [1; 16],
            kind: java_protocol::target::ENTITY_PLAYER,
            position: [0., -60., 3.],
            velocity: [0.; 3],
            pitch: 0.,
            yaw: 0.,
            head_yaw: 0.,
            data: 0,
        }),
    };
    let remove = || Event::Roster(wire::Packet::Remove(vec![[1; 16]]));
    assert!(state.admit(Envelope {
        generation: 7,
        event: update(true, WireMode::Survival)
    }));
    assert!(state.admit(Envelope {
        generation: 7,
        event: spawn()
    }));
    assert_eq!(state.player_entity_mode(99), Some(GameMode::Survival));
    assert!(state.admit(Envelope {
        generation: 7,
        event: remove()
    }));
    assert!(state.players.get(&[1; 16]).is_none());
    assert_eq!(state.player_entity_mode(99), Some(GameMode::Survival));
    // Re-adding the UUID creates another PlayerInfo; the live entity still holds the old one.
    assert!(state.admit(Envelope {
        generation: 7,
        event: update(true, WireMode::Spectator)
    }));
    assert_eq!(state.player_entity_mode(99), Some(GameMode::Survival));
    assert!(state.admit(Envelope {
        generation: 7,
        event: spawn()
    }));
    assert!(state.removed_player_modes.is_empty());
    assert_eq!(state.player_entity_mode(99), Some(GameMode::Spectator));
    assert!(state.admit(Envelope {
        generation: 7,
        event: remove()
    }));
    assert_eq!(state.player_entity_mode(99), Some(GameMode::Spectator));
    assert!(!state.admit(Envelope {
        generation: 6,
        event: remove()
    }));
    assert_eq!(state.removed_player_modes.len(), 1);
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Entities {
            dimension_epoch: 1,
            packet: entities::Packet::Remove(vec![99]),
        }
    }));
    assert!(state.removed_player_modes.is_empty());
    assert_eq!(state.player_entity_mode(99), None);
    // The current roster object updates normally; only a removed object's mode is frozen.
    assert!(state.admit(Envelope {
        generation: 7,
        event: update(true, WireMode::Survival)
    }));
    assert!(state.admit(Envelope {
        generation: 7,
        event: spawn()
    }));
    assert!(state.admit(Envelope {
        generation: 7,
        event: update(false, WireMode::Spectator)
    }));
    assert_eq!(state.player_entity_mode(99), Some(GameMode::Spectator));
    assert!(state.admit(Envelope {
        generation: 7,
        event: remove()
    }));
    let dimension = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    super::super::terrain::begin(&mut state, &dimension).unwrap();
    assert!(state.removed_player_modes.is_empty());
    assert_eq!(state.player_entity_mode(99), None);
    state.begin(8);
    assert!(state.removed_player_modes.is_empty());
}

#[test]
fn team_updates_are_session_fenced_and_retained_only_across_dimension_changes() {
    let mut state = super::super::tests_support::state();
    let event = || {
        Event::Roster(wire::Packet::Team(wire::teams::Packet {
            team: "red".into(),
            method: 0,
            members: vec!["Alice".into()],
            parameters: Some(wire::teams::Parameters {
                display: java_protocol::nbt::Value::String("Red".into()),
                prefix: java_protocol::nbt::Value::String("[R]".into()),
                suffix: java_protocol::nbt::Value::String("".into()),
                visibility: wire::teams::Visibility::Never,
                collision: 1,
                color: Some('c'),
                options: 2,
            }),
        }))
    };
    assert!(!state.admit(Envelope {
        generation: 6,
        event: event()
    }));
    assert!(state.players.team_parameters("Alice").is_none());
    assert!(state.admit(Envelope {
        generation: 7,
        event: event()
    }));
    assert!(!state.players.name_visible("Alice", "Viewer"));
    let p = state.players.team_parameters("Alice").unwrap();
    assert_eq!(p.color, Some('c'));
    assert_eq!(p.collision, domain::teams::Collision::Never);
    assert_eq!(p.prefix, world::NbtValue::String("[R]".into()));
    let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    super::super::terrain::begin(&mut state, &spawn).unwrap();
    assert!(!state.players.name_visible("Alice", "Viewer"));
    state.admit(Envelope {
        generation: 7,
        event: Event::Phase(Phase::Configuration),
    });
    assert!(state.players.team_parameters("Alice").is_none());
    assert!(!state.admit(Envelope {
        generation: 7,
        event: event()
    }));
    state.begin(8);
    assert!(!state.admit(Envelope {
        generation: 7,
        event: event()
    }));
}

#[test]
fn default_skin_matches_native_uuid_hash_and_negative_floor_mod_vectors() {
    // Independent DefaultPlayerSkin.get(UUID) output from the pinned client archive.
    for (uuid, texture, slim) in [
        (
            0u128,
            "minecraft:textures/entity/player/slim/alex.png",
            true,
        ),
        (1, "minecraft:textures/entity/player/slim/ari.png", true),
        (
            0xffff_ffff_0000_0000_0000_0000_0000_0000,
            "minecraft:textures/entity/player/wide/zuri.png",
            false,
        ),
        (
            0x123e_4567_e89b_12d3_a456_4266_1417_4000,
            "minecraft:textures/entity/player/slim/steve.png",
            true,
        ),
    ] {
        let skin = default_skin(uuid.to_be_bytes());
        assert_eq!(skin.texture.as_ref(), texture);
        assert_eq!(skin.slim, slim);
    }
}

#[test]
fn profile_authority_survives_dimension_replacement_but_not_session_or_configuration() {
    let mut state = super::super::tests_support::state();
    let event = || {
        Event::Roster(wire::Packet::Update(vec![wire::Update {
            uuid: [1; 16],
            profile: Some(wire::Profile {
                name: "FixturePeer".into(),
                properties: vec![],
            }),
            listed: Some(true),
            ..Default::default()
        }]))
    };
    assert!(!state.admit(Envelope {
        generation: 6,
        event: event()
    }));
    assert_eq!(state.players.iter().count(), 0);
    assert!(state.admit(Envelope {
        generation: 7,
        event: event()
    }));
    let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    super::super::terrain::begin(&mut state, &spawn).unwrap();
    assert_eq!(state.players.iter().count(), 1);
    assert!(state.players.get(&[1; 16]).unwrap().listed);
    state.admit(Envelope {
        generation: 7,
        event: Event::Phase(Phase::Configuration),
    });
    assert_eq!(state.players.iter().count(), 0);
    assert!(!state.admit(Envelope {
        generation: 7,
        event: event(),
    }));
    state = super::super::tests_support::state();
    assert!(state.admit(Envelope {
        generation: 7,
        event: event()
    }));
    state.begin(8);
    assert_eq!(state.players.iter().count(), 0);
    assert!(!state.admit(Envelope {
        generation: 7,
        event: event()
    }));
}
