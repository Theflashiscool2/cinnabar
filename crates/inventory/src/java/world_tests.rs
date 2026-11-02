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
