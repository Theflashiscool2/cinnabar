use super::*;
use java_protocol::session::{Envelope, Event};
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
