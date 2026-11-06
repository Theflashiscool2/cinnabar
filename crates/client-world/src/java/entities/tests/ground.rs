//! Ground stacks share entity lifetime and the existing item-appearance owner.
use super::*;
#[test]
fn authoritative_ground_metadata_updates_remove_reuse_and_scope_are_change_driven() {
    let mut s = state();
    let make_spawn = |lifetime| {
        let wire::Packet::Spawn(mut spawn) = spawn() else {
            unreachable!()
        };
        spawn.kind = java_protocol::target::ENTITY_ITEM;
        spawn.lifetime = lifetime;
        wire::Packet::Spawn(spawn)
    };
    let metadata = |count| {
        wire::Packet::Metadata(wire::metadata::Packet {
            id: 99,
            unsupported: None,
            entries: vec![wire::metadata::Entry {
                index: 8,
                serializer: 7,
                value: wire::metadata::Value::Item(Some(
                    java_protocol::session::inventory::Stack {
                        item: 55,
                        count,
                        added: [("minecraft:custom_name", serde_json::json!({"text":"Loot"}))]
                            .into(),
                        removed: Default::default(),
                    },
                )),
            }],
        })
    };
    assert!(!s.admit(event(1, metadata(5))));
    assert!(s.admit(event(1, make_spawn(1))));
    let first = s.entities.get(99).unwrap().identity;
    assert!(s.equipment.ground(first).is_none());
    assert!(s.admit(event(1, metadata(5))));
    let item = s.equipment.ground(first).unwrap();
    let revision = s.equipment.revision();
    assert_eq!(item.stack.as_ref().unwrap().item, "minecraft:dirt");
    assert_eq!(item.stack.as_ref().unwrap().count, 5);
    assert_eq!(
        item.stack.as_ref().unwrap().added["minecraft:custom_name"],
        serde_json::json!({"text":"Loot"})
    );
    assert!(s.admit(event(1, metadata(5))));
    assert_eq!(s.equipment.revision(), revision);
    assert!(s.admit(event(1, metadata(2))));
    assert!(s.equipment.revision() > revision);
    assert_eq!(
        s.equipment
            .ground(first)
            .unwrap()
            .stack
            .as_ref()
            .unwrap()
            .count,
        2
    );
    let mut stale = event(1, metadata(64));
    stale.generation = 6;
    assert!(!s.admit(stale));
    assert!(!s.admit(event(2, metadata(64))));
    assert_eq!(
        s.equipment
            .ground(first)
            .unwrap()
            .stack
            .as_ref()
            .unwrap()
            .count,
        2
    );
    assert!(s.admit(event(1, wire::Packet::Remove(vec![99]))));
    assert!(s.equipment.ground(first).is_none());
    assert!(s.admit(event(1, make_spawn(2))));
    let second = s.entities.get(99).unwrap().identity;
    assert_ne!(first, second);
    assert!(s.equipment.ground(second).is_none());
    assert!(s.admit(event(1, metadata(1))));
    assert!(s.equipment.ground(first).is_none());
    s.equipment.clear();
    assert!(s.equipment.ground(second).is_none());
}
