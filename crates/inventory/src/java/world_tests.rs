use super::*;
#[test]
fn world_rebind_preserves_only_same_player_storage_and_retires_menu_state() {
    let mut s = State::default();
    s.begin(7, 1);
    let stack = Stack {
        item: "minecraft:stone".into(),
        count: 5,
        added: Default::default(),
        removed: Default::default(),
    };
    s.admit(
        7,
        1,
        Update::PlayerSlot {
            slot: 2,
            stack: Some(stack.clone()),
        },
    )
    .unwrap();
    s.admit(7, 1, Update::Selected(2)).unwrap();
    s.opened = Some(Menu {
        container: 3,
        incarnation: 44,
        kind: "chest".into(),
        title: Value::Null,
        state: Some(10),
        slots: vec![],
        carried: Some(Some(stack)),
        properties: BTreeMap::new(),
    });
    s.error = Some("old menu failure".into());
    s.selection_receipt = Some(20);
    s.rebind_player(7, 2);
    assert_eq!(s.held().unwrap().as_ref().unwrap().count, 5);
    assert_eq!(s.selected, Some(2));
    assert!(s.opened.is_none() && s.error.is_none() && s.selection_receipt.is_none());
    assert!(s.player.as_ref().unwrap().carried.is_none());
    assert!(!s.accepts(7, 1));
    assert!(s.accepts(7, 2));
    s.rebind_player(8, 1);
    assert!(s.held().is_none());
    assert_eq!(s.selected, Some(0));
}

#[test]
fn personal_storage_pickup_uses_native_menu_indices_and_never_mutates_authority() {
    let mut s = State::default();
    s.begin(7, 1);
    let mut slots = vec![None; PLAYER_MENU_SIZE];
    slots[36] = Some(Stack {
        item: "minecraft:stone".into(),
        count: 7,
        added: Default::default(),
        removed: Default::default(),
    });
    s.admit(
        7,
        1,
        Update::Content {
            container: 0,
            state: 3,
            slots,
            carried: None,
        },
    )
    .unwrap();
    let id = s.active_identity().unwrap();
    let p = s
        .plan_pickup(id, 36, pickup::Button::Right, |_| Ok(64))
        .unwrap();
    assert_eq!(p.carried.as_ref().unwrap().count, 4);
    assert_eq!(p.changed[&36].as_ref().unwrap().count, 3);
    assert_eq!(s.slot(0, 36).unwrap().as_ref().unwrap().count, 7);
    assert!(
        s.plan_pickup(id, 5, pickup::Button::Left, |_| Ok(64))
            .is_err(),
        "armor needs specialized slot rules"
    );
    s.begin(7, 2);
    assert!(
        s.plan_pickup(id, 36, pickup::Button::Left, |_| Ok(64))
            .is_err()
    );
}
