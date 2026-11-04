//! Registry/world retirement across a configuration round trip on one session.
use super::*;
use java_protocol::{nbt::Value, session::facts::RegistryEntry};
fn receive(state: &mut WorldState, event: Event) -> bool {
    state.admit(Envelope {
        generation: 7,
        event,
    })
}
#[test]
fn reconfiguration_replaces_registry_ids_and_rejects_retired_world_receipts() {
    let mut state = tests_support::state();
    let mut facts = state.bootstrap.as_ref().unwrap().clone();
    facts.configuration.registries.insert(
        "minecraft:worldgen/biome".into(),
        vec![RegistryEntry {
            name: "fixture:new_biome".into(),
            value: Value::Compound(
                [
                    ("temperature".into(), Value::Float(0.5)),
                    ("downfall".into(), Value::Float(0.7)),
                    (
                        "effects".into(),
                        Value::Compound([("water_color".into(), Value::Int(0x123456))].into()),
                    ),
                ]
                .into(),
            ),
        }],
    );
    let Value::Compound(dimension) = &mut facts
        .configuration
        .registries
        .get_mut("minecraft:dimension_type")
        .unwrap()[0]
        .value
    else {
        panic!()
    };
    dimension.insert("min_y".into(), Value::Int(0));
    dimension.insert("height".into(), Value::Int(256));
    state.vitals = Some(java_protocol::session::facts::Vitals {
        health: 1.,
        food: 0,
        saturation: 0.,
    });
    state.death = Some((42, Value::String("old".into())));
    assert!(receive(
        &mut state,
        Event::Ping {
            sequence: 1,
            id: 17
        }
    ));
    assert!(state.ping_command().is_some());
    assert!(receive(&mut state, Event::Phase(Phase::Configuration)));
    assert!(state.bootstrap.is_none());
    assert!(state.terrain.dimension().is_none());
    assert!(state.player_motion.is_none());
    assert!(state.biomes.is_empty());
    assert!(state.vitals.is_none() && state.death.is_none());
    assert!(state.ping_command().is_none());
    assert!(!receive(
        &mut state,
        Event::BlockAck {
            dimension_epoch: 1,
            sequence: 2
        }
    ));
    assert!(receive(&mut state, Event::Phase(Phase::Play)));
    assert!(receive(&mut state, Event::Bootstrap(facts)));
    let d = state.terrain.dimension().unwrap();
    assert_eq!((d.session, d.epoch, d.min_y, d.height), (7, 2, 0, 256));
    assert_eq!(state.biomes.len(), 1);
    assert_eq!(state.biomes[0].name, "fixture:new_biome");
    assert_eq!(state.biomes[0].water_color, [0x12, 0x34, 0x56]);
    assert!(!receive(
        &mut state,
        Event::BlockAck {
            dimension_epoch: 1,
            sequence: 3
        }
    ));
    assert!(receive(
        &mut state,
        Event::BlockAck {
            dimension_epoch: 2,
            sequence: 4
        }
    ));
    // A same-connection ping ID may be reused; its sequence may not rewind.
    assert!(!receive(
        &mut state,
        Event::Ping {
            sequence: 1,
            id: 17
        }
    ));
    assert!(receive(
        &mut state,
        Event::Ping {
            sequence: 2,
            id: 17
        }
    ));
    let old = state.bootstrap.clone().unwrap();
    state.begin(8);
    assert!(!receive(&mut state, Event::Bootstrap(old)));
}
