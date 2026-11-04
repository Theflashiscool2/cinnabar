use super::*;
use java_protocol::session::{Envelope, Event};
#[test]
fn cooldown_facts_are_scoped_without_invalidating_inventory_clicks() {
    let mut s = crate::java::tests_support::state();
    let epoch = s.terrain.dimension().unwrap().epoch;
    let before = (s.inventory.revision, s.inventory.authority_revision);
    let event = |generation, dimension_epoch, ticks| Envelope {
        generation,
        event: Event::Inventory {
            dimension_epoch,
            incarnation: 0,
            packet: wire::Packet::Cooldown {
                group: "fixture:shared".into(),
                ticks,
            },
        },
    };
    assert!(s.admit(event(7, epoch, 40)));
    assert_eq!(
        (s.inventory.revision, s.inventory.authority_revision),
        before
    );
    assert!(!s.admit(event(6, epoch, 0)));
    assert!(!s.admit(event(7, epoch + 1, 0)));
    assert_eq!(s.inventory.cooldowns.remaining("fixture:shared"), 40);
    assert!(s.admit(event(7, epoch, 0)));
    assert!(!s.inventory.cooldowns.active("fixture:shared"));
}
#[test]
fn item_components_survive_authoritative_inventory_admission_and_replacement() {
    let mut state = crate::java::tests_support::state();
    let epoch = state.terrain.dimension().unwrap().epoch;
    state.inventory.begin(7, epoch);
    let modifiers = serde_json::json!({"modifiers":[{
        "attribute":"minecraft:attack_damage",
        "modifier":{"id":"fixture:damage","amount":2.5,"operation":"add"},
        "slot":"main_hand", "display":{"type":"hidden","component":null}
    }]});
    let item = wire::Stack {
        item: 55,
        count: 1,
        added: [("minecraft:attribute_modifiers", modifiers.clone())].into(),
        removed: Default::default(),
    };
    let update = |generation, item| Envelope {
        generation,
        event: Event::Inventory {
            incarnation: 0,
            dimension_epoch: epoch,
            packet: wire::Packet::PlayerSlot {
                slot: 0,
                stack: Some(item),
            },
        },
    };
    assert!(state.admit(update(7, item.clone())));
    assert_eq!(
        state.inventory.held().unwrap().as_ref().unwrap().added["minecraft:attribute_modifiers"],
        modifiers
    );
    let removed = wire::Stack {
        added: Default::default(),
        removed: ["minecraft:attribute_modifiers"].into(),
        ..item
    };
    assert!(!state.admit(update(6, removed.clone())));
    assert!(
        state
            .inventory
            .held()
            .unwrap()
            .as_ref()
            .unwrap()
            .added
            .contains_key("minecraft:attribute_modifiers")
    );
    assert!(state.admit(update(7, removed)));
    let held = state.inventory.held().unwrap().as_ref().unwrap();
    assert!(held.added.is_empty());
    assert!(held.removed.contains("minecraft:attribute_modifiers"));
    state.begin(8);
    assert!(state.inventory.held().is_none());
}
#[test]
fn inventory_requires_current_session_dimension_and_valid_identity() {
    let mut state = crate::java::tests_support::state();
    let epoch = state.terrain.dimension().unwrap().epoch;
    let message = |generation, dimension_epoch| Envelope {
        generation,
        event: Event::Inventory {
            incarnation: 0,
            dimension_epoch,
            packet: wire::Packet::Selected(2),
        },
    };
    assert!(!state.admit(message(6, epoch)));
    assert!(!state.admit(message(7, epoch + 1)));
    assert!(state.admit(message(7, epoch)));
    assert_eq!(state.inventory.selected, Some(2));
    let invalid = wire::Stack {
        item: u32::MAX,
        count: 1,
        added: Default::default(),
        removed: Default::default(),
    };
    assert!(!admit(
        &mut state,
        epoch,
        0,
        wire::Packet::PlayerSlot {
            slot: 2,
            stack: Some(invalid)
        }
    ));
    assert!(state.inventory.held().is_none());
    state.begin(8);
    assert_eq!(state.inventory.selected, None);
}

#[test]
fn native_opening_identity_and_local_close_receipts_do_not_invent_server_item_changes() {
    let mut state = crate::java::tests_support::state();
    let epoch = state.terrain.dimension().unwrap().epoch;
    state.inventory.begin(7, epoch);
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Inventory {
            dimension_epoch: epoch,
            incarnation: 44,
            packet: wire::Packet::Open {
                container: 7,
                kind: "GENERIC_9X3",
                title: java_protocol::nbt::Value::String("Chest".into()),
            },
        }
    }));
    assert_eq!(state.inventory.opened.as_ref().unwrap().incarnation, 44);
    let authority = state.inventory.authority_revision;
    assert!(!state.admit(Envelope {
        generation: 7,
        event: Event::InventorySent {
            dimension_epoch: epoch,
            incarnation: 43,
            sequence: 1,
            container: 7,
            closed: true,
            written: true,
        }
    }));
    assert!(state.inventory.opened.is_some());
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::InventorySent {
            dimension_epoch: epoch,
            incarnation: 44,
            sequence: 2,
            container: 7,
            closed: false,
            written: true,
        }
    }));
    assert_eq!(state.inventory.authority_revision, authority);
    assert!(state.inventory.opened.is_some());
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::InventorySent {
            dimension_epoch: epoch,
            incarnation: 44,
            sequence: 3,
            container: 7,
            closed: true,
            written: true,
        }
    }));
    assert!(state.inventory.opened.is_none());
    assert_eq!(state.inventory.authority_revision, authority);
}
