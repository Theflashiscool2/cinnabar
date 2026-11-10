//! Native collect packets affect ground presentation, never transactional player storage.
use super::*;
fn ground() -> crate::java::WorldState {
    let mut state = state();
    state.player_motion = Some(Default::default());
    let wire::Packet::Spawn(mut item) = spawn() else {
        unreachable!()
    };
    item.kind = java_protocol::target::ENTITY_ITEM;
    assert!(state.admit(event(1, wire::Packet::Spawn(item))));
    assert!(state.admit(event(
        1,
        wire::Packet::Metadata(wire::metadata::Packet {
            id: 99,
            unsupported: None,
            entries: vec![wire::metadata::Entry {
                    index: 8,
                    serializer: 7,
                    value: wire::metadata::Value::Item(Some(
                        java_protocol::session::inventory::Stack {
                            item: java_protocol::target::ITEM_TYPES
                                .iter()
                                .position(|name| *name == "minecraft:iron_ingot")
                                .unwrap() as u32,
                            count: 5,
                            added: Default::default(),
                            removed: Default::default(),
                        }
                    )),
                }],
        })
    )));
    state
}
fn take(amount: i32, item_lifetime: u64) -> wire::Packet {
    wire::Packet::TakeItem {
        item: 99,
        item_lifetime,
        collector: 42,
        collector_lifetime: 0,
        amount,
    }
}
#[test]
fn pickup_snapshots_pre_shrink_stack_and_never_awards_inventory() {
    let mut state = ground();
    let source = state.entities.get(99).unwrap().identity;
    let inventory = state.inventory.revision;
    assert!(!state.admit(event(1, take(2, 0))));
    assert_eq!(
        state
            .equipment
            .ground(source)
            .unwrap()
            .stack
            .as_ref()
            .unwrap()
            .count,
        5
    );
    assert!(state.admit(event(1, take(2, 1))));
    assert_eq!(
        state
            .equipment
            .ground(source)
            .unwrap()
            .stack
            .as_ref()
            .unwrap()
            .count,
        3
    );
    let pickup = state.equipment.pickups().next().unwrap();
    assert_ne!(pickup.identity, source);
    assert_eq!(pickup.ground.stack.as_ref().unwrap().count, 5);
    assert_eq!(state.inventory.revision, inventory);
    let midpoint = [0., f64::from(1.62_f32) / 2., 0.];
    state.equipment.initialize_pickup_targets(Some(midpoint));
    for expected in [1, 1, 0] {
        state
            .equipment
            .tick_pickups(&state.entities, Some(midpoint));
        assert_eq!(state.equipment.pickups().count(), expected);
    }
    assert_eq!(
        state
            .equipment
            .ground(source)
            .unwrap()
            .stack
            .as_ref()
            .unwrap()
            .count,
        3
    );
}
#[test]
fn depleted_source_keeps_frozen_visual_but_explicit_despawn_reuse_and_scope_retire_it() {
    let mut state = ground();
    let source = state.entities.get(99).unwrap().identity;
    assert!(state.admit(event(1, take(5, 1))));
    assert!(state.entities.get(99).is_none());
    assert!(state.equipment.ground(source).is_none());
    assert_eq!(state.equipment.pickups().count(), 1);
    // The routine server removal after depletion cannot erase the native render snapshot.
    state.admit(event(1, wire::Packet::Remove(vec![99])));
    assert_eq!(state.equipment.pickups().count(), 1);
    state = ground();
    assert!(state.admit(event(1, take(2, 1))));
    assert!(state.admit(event(1, wire::Packet::Remove(vec![99]))));
    assert_eq!(state.equipment.pickups().count(), 0);
    state = ground();
    assert!(state.admit(event(1, take(5, 1))));
    let wire::Packet::Spawn(mut replacement) = spawn() else {
        unreachable!()
    };
    replacement.kind = java_protocol::target::ENTITY_ITEM;
    replacement.lifetime = 2;
    assert!(state.admit(event(1, wire::Packet::Spawn(replacement))));
    assert_eq!(state.equipment.pickups().count(), 0);
    assert!(!state.admit(event(1, take(1, 1))));
    state = ground();
    assert!(state.admit(event(1, take(5, 1))));
    state.begin(8);
    assert_eq!(state.equipment.pickups().count(), 0);
    assert!(!state.admit(event(1, take(5, 1))));
}
#[test]
fn stale_collector_cannot_attach_a_flight_to_its_replacement() {
    let mut state = ground();
    let wire::Packet::Spawn(mut collector) = spawn() else {
        unreachable!()
    };
    collector.id = 100;
    collector.lifetime = 2;
    assert!(state.admit(event(1, wire::Packet::Spawn(collector))));
    assert!(state.admit(event(
        1,
        wire::Packet::TakeItem {
            item: 99,
            item_lifetime: 1,
            collector: 100,
            collector_lifetime: 1,
            amount: 1
        }
    )));
    assert_eq!(state.equipment.pickups().count(), 0);
    assert!(state.admit(event(
        1,
        wire::Packet::TakeItem {
            item: 99,
            item_lifetime: 1,
            collector: 100,
            collector_lifetime: 2,
            amount: 1
        }
    )));
    assert_eq!(state.equipment.pickups().count(), 1);
    assert!(state.admit(event(1, wire::Packet::Remove(vec![100]))));
    assert_eq!(state.equipment.pickups().count(), 0);
}
