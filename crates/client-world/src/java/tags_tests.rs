use super::*;
use java_protocol::session::facts::Tags;

fn tags(registry: &str, tag: &str, ids: &[i32]) -> Tags {
    [(registry.into(), [(tag.into(), ids.to_vec())].into())].into()
}

#[test]
fn tag_replacement_is_registry_scoped_and_rejects_retired_sessions() {
    let mut state = tests_support::state();
    for (registry, tag, ids) in [
        ("minecraft:block", "fixture:old", &[1][..]),
        ("minecraft:item", "fixture:kept", &[2][..]),
    ] {
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Tags(tags(registry, tag, ids))
        }));
    }
    assert!(!state.admit(Envelope {
        generation: 6,
        event: Event::Tags(tags("minecraft:block", "fixture:stale", &[9]))
    }));
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags("minecraft:block", "fixture:new", &[3]))
    }));
    let current = &state.bootstrap.as_ref().unwrap().configuration.tags;
    assert_eq!(current["minecraft:block"].len(), 1);
    assert_eq!(current["minecraft:block"]["fixture:new"], [3]);
    assert_eq!(current["minecraft:item"]["fixture:kept"], [2]);
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Tags([("minecraft:block".into(), Default::default())].into())
    }));
    assert!(state.bootstrap.as_ref().unwrap().configuration.tags["minecraft:block"].is_empty());
    assert_eq!(state.deferred_packets, 0);

    state.begin(8);
    assert!(!state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags("minecraft:block", "fixture:stale", &[9]))
    }));
    assert!(!state.admit(Envelope {
        generation: 8,
        event: Event::Tags(tags("minecraft:block", "fixture:early", &[9]))
    }));
    assert!(state.bootstrap.is_none());
}

#[test]
fn configuration_transition_discards_previous_tags() {
    let mut state = tests_support::state();
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags("minecraft:item", "fixture:old", &[2]))
    }));
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Phase(Phase::Configuration)
    }));
    assert!(state.bootstrap.is_none());
    assert!(!state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags("minecraft:item", "fixture:late", &[2]))
    }));
}

#[test]
fn respawn_keeps_session_tags_and_accepts_new_replacements() {
    let mut state = tests_support::state();
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags("minecraft:item", "fixture:kept", &[2])),
    }));
    let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Respawn {
            spawn,
            keep_attribute_modifiers: false,
            keep_metadata: false,
        },
    }));
    assert_eq!(state.dimension_epoch, 2);
    assert_eq!(
        state.bootstrap.as_ref().unwrap().configuration.tags["minecraft:item"]["fixture:kept"],
        [2]
    );
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags("minecraft:item", "fixture:new", &[3])),
    }));
    let items = &state.bootstrap.as_ref().unwrap().configuration.tags["minecraft:item"];
    assert_eq!(items.len(), 1);
    assert_eq!(items["fixture:new"], [3]);
}
