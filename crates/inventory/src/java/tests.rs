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
            incarnation: s.incarnation + 1,
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
                slots: vec![item("minecraft:dirt", 1); PLAYER_MENU_SIZE + 1],
                carried: item("minecraft:torch", 2)
            }
        )
        .is_err()
    );
    assert!(s.held().is_none());
    assert_eq!(s.player.as_ref().unwrap().state, None);
    assert_eq!(s.player.as_ref().unwrap().carried, None);
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

#[test]
fn local_selection_receipts_and_server_corrections_share_one_ordered_inventory() {
    let mut state = State::default();
    state.begin(7, 1);
    assert!(!state.selection_sent(6, 1, 1, 8));
    assert!(!state.selection_sent(7, 2, 1, 8));
    state.admit(7, 1, Update::Selected(4)).unwrap();
    assert!(state.selection_sent(7, 1, 1, 8));
    assert_eq!(state.selected, Some(8));
    state.admit(7, 1, Update::Selected(2)).unwrap();
    assert!(!state.selection_sent(7, 1, 1, 8));
    assert_eq!(state.selected, Some(2));
    state.begin(8, 1);
    assert_eq!(state.selection_receipt, None);
    assert_eq!(state.selected, Some(0));
}

#[test]
fn personal_reserved_clicks_are_observed_without_storage_predictions_or_cross_menu_reuse() {
    let mut s = state();
    let id = s.active_identity().unwrap();
    assert!(s.observe_click_slot(id, 5).is_err());
    apply(
        &mut s,
        Update::Content {
            container: 0,
            state: 17,
            slots: vec![None; 46],
            carried: item("minecraft:dirt", 4),
        },
    );
    let before = serde_json::to_value(&s).unwrap();
    for slot in 0..9 {
        let (revision, carried) = s.observe_click_slot(id, slot).unwrap();
        assert_eq!(revision, 17);
        assert_eq!(carried, &item("minecraft:dirt", 4));
        assert!(
            s.plan_pickup(id, slot, pickup::Button::Left, |_| Ok(64))
                .is_err()
        );
    }
    assert_eq!(serde_json::to_value(&s).unwrap(), before);
    assert!(s.observe_click_slot(id, 46).is_err());
    open(&mut s, 4);
    assert!(s.observe_click_slot(id, 5).is_err());
    let foreign = s.active_identity().unwrap();
    assert!(s.observe_click_slot(foreign, 5).is_err()); // unknown contents
    s.begin(7, 3);
    assert!(s.observe_click_slot(foreign, 5).is_err());
}

#[test]
fn unsupported_merchant_retains_real_slot_authority_but_never_admits_storage_clicks() {
    let mut s = state();
    apply(
        &mut s,
        Update::Open {
            incarnation: 1,
            container: 7,
            kind: "MERCHANT".into(),
            title: Value::Null,
        },
    );
    assert_eq!(s.opened.as_ref().unwrap().len(), 39);
    let id = s.opened_identity().unwrap();
    let mut slots = vec![None; 39];
    slots[0] = item("minecraft:emerald", 2);
    slots[2] = item("minecraft:bread", 3);
    slots[30] = item("minecraft:dirt", 5);
    apply(
        &mut s,
        Update::Content {
            container: 7,
            state: 4,
            slots,
            carried: item("minecraft:stone", 1),
        },
    );
    assert_eq!(s.held(), Some(&item("minecraft:dirt", 5)));
    assert!(s.observe_click_slot(id, 2).is_err());
    assert!(s.observe_storage_slot(id, 30).is_err());
    assert!(s.observe_carried(id).is_err());
    assert!(!s.record_receipt(pickup::Receipt {
        identity: pickup::MenuIdentity {
            incarnation: 0,
            ..id
        },
        sequence: 1,
        closed: true,
        written: true
    }));
    assert!(s.record_receipt(pickup::Receipt {
        identity: id,
        sequence: 2,
        closed: true,
        written: true
    }));
    assert!(s.opened.is_none());
    assert_eq!(s.held(), Some(&item("minecraft:dirt", 5)));
}

#[test]
fn invalid_authority_reports_original_failure_without_accepting_a_click() {
    let mut s = state();
    let id = s.active_identity().unwrap();
    let error = s
        .admit(
            7,
            2,
            Update::Content {
                container: 0,
                state: 0,
                slots: vec![None; PLAYER_MENU_SIZE + 1],
                carried: None,
            },
        )
        .unwrap_err();
    let observed = s.observe_storage_slot(id, 36).unwrap_err();
    assert_eq!(
        observed,
        format!("Java inventory authority invalid: {error}")
    );
    assert!(s.held().is_none());
    let stale = pickup::MenuIdentity { epoch: 1, ..id };
    assert_eq!(
        s.observe_storage_slot(stale, 36).unwrap_err(),
        "Java container lifetime is no longer current"
    );
}

#[test]
fn native_content_prefix_updates_hotbar_and_preserves_omitted_offhand() {
    let mut s = state();
    let mut prefix = vec![None; OFFHAND_MENU_SLOT];
    prefix[HOTBAR_START] = item("minecraft:oak_planks", 16);
    apply(
        &mut s,
        Update::Content {
            container: 0,
            state: 3,
            slots: prefix.clone(),
            carried: None,
        },
    );
    assert!(s.error.is_none());
    assert_eq!(s.held(), Some(&item("minecraft:oak_planks", 16)));
    assert_eq!(
        s.slot(0, OFFHAND_MENU_SLOT),
        None,
        "omitted does not mean empty"
    );
    apply(
        &mut s,
        Update::Slot {
            container: 0,
            state: 4,
            slot: OFFHAND_MENU_SLOT,
            stack: item("minecraft:shield", 1),
        },
    );
    prefix[HOTBAR_START] = item("minecraft:oak_planks", 15);
    apply(
        &mut s,
        Update::Content {
            container: 0,
            state: 5,
            slots: prefix,
            carried: None,
        },
    );
    assert_eq!(
        s.slot(0, OFFHAND_MENU_SLOT),
        Some(&item("minecraft:shield", 1))
    );
    assert_eq!(s.held(), Some(&item("minecraft:oak_planks", 15)));
    open(&mut s, 2);
    apply(
        &mut s,
        Update::Content {
            container: 2,
            state: 6,
            slots: vec![item("minecraft:paper", 1)],
            carried: None,
        },
    );
    let id = s.active_identity().unwrap();
    assert_eq!(s.observe_storage_slot(id, 0).unwrap().0, 6);
    assert!(
        s.slot(2, 1).is_none(),
        "unreported server menu slot stays unknown"
    );
    assert!(
        !s.admit(
            7,
            1,
            Update::Content {
                container: 2,
                state: 7,
                slots: vec![],
                carried: None
            }
        )
        .unwrap()
    );
}
