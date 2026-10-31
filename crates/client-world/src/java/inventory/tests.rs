use super::*;
use java_protocol::session::{Envelope, Event};
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
        wire::Packet::PlayerSlot {
            slot: 2,
            stack: Some(invalid)
        }
    ));
    assert!(state.inventory.held().is_none());
    state.begin(8);
    assert_eq!(state.inventory.selected, None);
}
