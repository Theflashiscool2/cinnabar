use super::*;
fn item(name: &str, count: u32) -> Slot {
    Some(Stack {
        item: name.into(),
        count,
        added: BTreeMap::new(),
        removed: BTreeSet::new(),
    })
}
fn state() -> State {
    let mut s = State::default();
    s.begin(7, 2);
    s
}
fn apply(s: &mut State, update: Update) {
    assert!(s.admit(7, 2, update).unwrap());
}
fn open(s: &mut State, id: i32) {
    apply(
        s,
        Update::Open {
            container: id,
            kind: "GENERIC_9X3".into(),
            title: Value::Null,
        },
    );
}
#[test]
fn unknown_empty_and_held_inventory_are_distinct_and_follow_native_slot_ownership() {
    let mut s = state();
    assert!(s.held().is_none());
    let mut slots = vec![None; 46];
    slots[36] = item("minecraft:dirt", 64);
    slots[5] = item("minecraft:iron_helmet", 1);
    slots[45] = item("minecraft:shield", 1);
    apply(
        &mut s,
        Update::Content {
            container: 0,
            state: 10,
            slots,
            carried: None,
        },
    );
    assert_eq!(s.held(), Some(&item("minecraft:dirt", 64)));
    assert_eq!(s.storage[39], Cell::Known(item("minecraft:iron_helmet", 1)));
    assert_eq!(s.storage[40], Cell::Known(item("minecraft:shield", 1)));
    apply(&mut s, Update::Selected(1));
    assert_eq!(s.held(), Some(&None));
    apply(
        &mut s,
        Update::PlayerSlot {
            slot: 1,
            stack: item("minecraft:stone", 2),
        },
    );
    assert_eq!(s.slot(0, 37), Some(&item("minecraft:stone", 2)));
}
#[test]
fn container_views_share_storage_and_ordered_state_ids_can_wrap() {
    let mut s = state();
    open(&mut s, 4);
    let mut slots = vec![None; 63];
    slots[54] = item("minecraft:dirt", 5);
    apply(
        &mut s,
        Update::Content {
            container: 4,
            state: 32767,
            slots,
            carried: item("minecraft:stone", 1),
        },
    );
    assert_eq!(s.held(), Some(&item("minecraft:dirt", 5)));
    assert_eq!(s.slot(0, 36), s.slot(4, 54));
    apply(
        &mut s,
        Update::Slot {
            container: 4,
            state: 0,
            slot: 54,
            stack: item("minecraft:dirt", 4),
        },
    );
    assert_eq!(s.menu(4).unwrap().state, Some(0));
    assert_eq!(s.slot(0, 36), Some(&item("minecraft:dirt", 4)));
    apply(
        &mut s,
        Update::Slot {
            container: 0,
            state: 2,
            slot: 36,
            stack: None,
        },
    );
    assert_eq!(s.slot(4, 54), Some(&None));
    apply(
        &mut s,
        Update::PlayerSlot {
            slot: 0,
            stack: item("minecraft:stone", 9),
        },
    );
    assert_eq!(s.slot(4, 54), s.held());
}
#[test]
fn invalid_snapshots_are_atomic_and_replacement_retires_containers_and_cursor() {
    let mut s = state();
    open(&mut s, 4);
    let first = s.opened.as_ref().unwrap().incarnation;
    assert!(
        s.admit(
            7,
            2,
            Update::Content {
                container: 0,
                state: 1,
                slots: vec![None; 45],
                carried: None
            }
        )
        .is_err()
    );
    assert!(s.held().is_none());
    assert_eq!(s.player.as_ref().unwrap().state, None);
    open(&mut s, 5);
    assert!(s.opened.as_ref().unwrap().incarnation > first);
    assert!(!s.admit(7, 2, Update::Close(4)).unwrap());
    assert!(
        !s.admit(
            7,
            2,
            Update::Slot {
                container: 4,
                state: 9,
                slot: 0,
                stack: item("minecraft:dirt", 1)
            }
        )
        .unwrap()
    );
    assert!(!s.admit(6, 2, Update::Selected(3)).unwrap());
    assert!(!s.admit(7, 1, Update::Selected(3)).unwrap());
    s.begin(7, 3);
    assert!(s.opened.is_none() && s.held().is_none());
    assert_eq!(s.player.as_ref().unwrap().carried, None);
    s.begin(8, 1);
    assert!(!s.admit(7, 3, Update::Selected(3)).unwrap());
}
