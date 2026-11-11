//! Remote native use flags and appearance slots join only within one admitted entity lifetime.
use super::*;
fn equip(item: Option<&str>) -> wire::Packet {
    wire::Packet::Equipment(wire::equipment::Packet {
        id: 99,
        lifetime: 1,
        slots: [(
            0,
            item.map(|item| java_protocol::session::inventory::Stack {
                item: java_protocol::target::ITEM_TYPES
                    .iter()
                    .position(|name| *name == item)
                    .unwrap() as u32,
                count: 1,
                added: Default::default(),
                removed: Default::default(),
            }),
        )]
        .into(),
    })
}
fn flags(value: u8) -> wire::Packet {
    let spec = java_protocol::target::METADATA_PLAYER
        .iter()
        .find(|spec| spec.name == "DATA_LIVING_ENTITY_FLAGS")
        .unwrap();
    wire::Packet::Metadata(wire::metadata::Packet {
        id: 99,
        unsupported: None,
        entries: vec![wire::metadata::Entry {
            index: spec.index,
            serializer: spec.serializer,
            value: wire::metadata::Value::Byte(value),
        }],
    })
}
#[test]
fn remote_bow_use_requires_flag_receipt_and_preserves_duration_until_replacement_tick() {
    use world::java::entities::animation::PlayerUse;
    let mut state = state();
    let wire::Packet::Spawn(mut player) = spawn() else {
        unreachable!()
    };
    player.kind = java_protocol::target::ENTITY_PLAYER;
    assert!(state.admit(event(1, wire::Packet::Spawn(player))));
    let source = state.entities.get(99).unwrap().identity;
    assert!(state.admit(event(1, flags(1))));
    assert!(
        state.entities.get(99).unwrap().visual.remote_use.is_none(),
        "unknown equipment must not invent use"
    );
    assert!(state.admit(event(1, equip(Some("minecraft:bow")))));
    assert!(
        state.entities.get(99).unwrap().visual.remote_use.is_none(),
        "equipment is not a start-use event"
    );
    assert!(state.admit(event(1, flags(1))));
    let duration = state
        .entities
        .get(99)
        .unwrap()
        .visual
        .remote_use
        .as_ref()
        .unwrap()
        .remaining
        .unwrap();
    assert_eq!(
        super::super::super::equipment::player_use_pose(
            &state.entities.get(99).unwrap().visual,
            state.equipment.get(source)
        ),
        Some(PlayerUse::Bow { left: false })
    );
    state.entities.tick_presentation();
    let wire::Packet::Equipment(mut wear) = equip(Some("minecraft:bow")) else {
        unreachable!()
    };
    wear.slots
        .get_mut(&0)
        .unwrap()
        .as_mut()
        .unwrap()
        .added
        .insert("minecraft:damage", serde_json::json!(3));
    assert!(state.admit(event(1, wire::Packet::Equipment(wear))));
    assert!(state.admit(event(1, flags(1))));
    assert_eq!(
        state
            .entities
            .get(99)
            .unwrap()
            .visual
            .remote_use
            .as_ref()
            .unwrap()
            .remaining,
        Some(duration - 1),
        "same item and repeated flags must not restart native remaining time"
    );
    state
        .entities
        .update((7, 1), 99, |entity| {
            entity.visual.remote_use.as_mut().unwrap().remaining = Some(1);
            Ok(())
        })
        .unwrap();
    state.entities.tick_presentation();
    assert_eq!(
        super::super::super::equipment::player_use_pose(
            &state.entities.get(99).unwrap().visual,
            state.equipment.get(source)
        ),
        None
    );
    assert!(state.admit(event(1, flags(1))));
    assert_eq!(
        state
            .entities
            .get(99)
            .unwrap()
            .visual
            .remote_use
            .as_ref()
            .unwrap()
            .remaining,
        Some(0)
    );
    assert!(state.admit(event(1, flags(0))));
    assert!(state.admit(event(1, flags(1))));
    assert!(state.admit(event(1, equip(Some("minecraft:shield")))));
    state.entities.tick_presentation();
    assert!(state.entities.get(99).unwrap().visual.remote_use.is_none());
    assert_eq!(
        super::super::super::equipment::player_use_pose(
            &state.entities.get(99).unwrap().visual,
            state.equipment.get(source)
        ),
        None
    );
    assert!(state.admit(event(1, flags(1))));
    assert_eq!(
        super::super::super::equipment::player_use_pose(
            &state.entities.get(99).unwrap().visual,
            state.equipment.get(source)
        ),
        Some(PlayerUse::Block { left: false })
    );
    assert!(state.admit(event(1, flags(0))));
    assert!(state.entities.get(99).unwrap().visual.remote_use.is_none());
    assert!(state.admit(event(1, flags(1))));
    assert!(state.admit(event(1, wire::Packet::Remove(vec![99]))));
    assert!(state.equipment.get(source).is_none());
    assert!(state.entities.get(99).is_none());
}
