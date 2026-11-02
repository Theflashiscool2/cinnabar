use super::*;
use java_protocol::session::{Envelope, Event, Phase};

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
