use super::*;
#[test]
fn native_hotbar_keeps_counts_without_art_and_never_reinterprets_ids() {
    let icon = IconRef {
        color_mask: None,
        page: 2,
        uv: [0, 0, 16, 16],
        glint: false,
    };
    let mut native = HudItems::default();
    native.hotbar[0] = HudItem {
        known: true,
        count: 37,
        icon: Some(icon),
        durability: Some(0.25),
    };
    native.hotbar[1] = HudItem {
        known: true,
        count: 5,
        ..Default::default()
    };
    native.offhand = HudItem {
        known: true,
        count: 2,
        icon: Some(icon),
        ..Default::default()
    };
    let mut icons = Vec::new();
    let (hotbar, offhand) = native.bind(&mut icons, Some(1));
    assert_eq!(hotbar[0].count, 37);
    assert_eq!(hotbar[0].durability, Some(0.25));
    assert_eq!(hotbar[1].count, 5);
    assert!(hotbar[1].icon.is_none() && hotbar[1].selected);
    assert!(hotbar.iter().all(|s| s.id_aux.is_none()));
    assert_eq!(icons, [icon, icon]);
    let model = json_ui::HudModel {
        hotbar,
        offhand,
        ..Default::default()
    };
    assert!(native.matches(&model, &icons, Some(1)));
    native.hotbar[0].count = 3;
    assert!(!native.matches(&model, &icons, Some(1)));
    native.hotbar[0].count = 37;
    assert!(!native.matches(&model, &icons, Some(0)));
}

fn hud(session: u64, epoch: u64) -> world::java::hud::Snapshot {
    world::java::hud::Snapshot {
        session,
        epoch,
        mode: Some(world::java::hud::GameMode::Survival),
        hardcore: false,
        selected_slot: Some(0),
        health: None,
        maximum_health: None,
        food: None,
        saturation: None,
        armor: None,
        experience: None,
    }
}
fn stack(count: u32) -> Slot {
    Some(inventory::java::Stack {
        item: "minecraft:stone".into(),
        count,
        added: Default::default(),
        removed: Default::default(),
    })
}
#[test]
fn authority_snapshots_alias_open_menus_and_retire_on_world_or_account_change() {
    use inventory::java::Update;
    let mut world = client_world::java::WorldState::default();
    world.begin(7);
    world.inventory.begin(7, 1);
    let mut ui = UiRuntime::new(7);
    assert!(ui.publish_java_hud(hud(7, 1)));
    world
        .inventory
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: stack(37),
            },
        )
        .unwrap();
    assert!(ui.publish_java_inventory(&world));
    assert_eq!(ui.session.java_inventory.hotbar[0], Some(stack(37)));
    assert!(ui.session.java_inventory.hotbar[1].is_none());
    world
        .inventory
        .admit(
            7,
            1,
            Update::Open {
                incarnation: 1,
                container: 2,
                kind: "GENERIC_9X3".into(),
                title: serde_json::json!({"String":"Test chest"}),
            },
        )
        .unwrap();
    let mut slots = vec![None; 63];
    slots[54] = stack(5);
    slots[0] = stack(17);
    world
        .inventory
        .admit(
            7,
            1,
            Update::Content {
                container: 2,
                state: 4,
                slots,
                carried: stack(3),
            },
        )
        .unwrap();
    assert!(ui.publish_java_inventory(&world));
    assert_eq!(ui.session.java_inventory.hotbar[0], Some(stack(5)));
    assert_eq!(ui.java_container_slots(), Some(Some(27)));
    assert_eq!(ui.java_container_title().as_deref(), Some("Test chest"));
    assert!(ui.inventory_open() && ui.java_inventory_holding());
    ui.java_pointer_action(super::super::inventory_drag::PointerAction::Click(
        ui_screens::container_actions::InventoryCellHit::Player(0),
    ));
    let input = ui.next_java_inventory_input().unwrap();
    assert_eq!(input.identity.container, 2);
    assert!(matches!(input.intent, Intent::Pickup { slot: 54, .. }));
    assert_eq!(
        world.inventory.slot(2, 54),
        Some(&stack(5)),
        "click input never confirms item movement"
    );
    assert!(ui.publish_java_hud(hud(7, 2)));
    assert!(ui.next_java_inventory_input().is_none());
    assert!(ui.session.java_inventory.hotbar[0].is_none());
    assert!(
        !ui.publish_java_inventory(&world),
        "old epoch cannot republish old items"
    );
    ui.begin_session(8);
    assert!(!ui.publish_java_inventory(&world));
}
#[test]
fn server_text_is_bounded_and_literal_not_executed() {
    let text = component_title(&serde_json::json!({"Compound": {
        "text":{"String":"Menu "},"extra":{"List":[{"String":"世界"}]}
    }}));
    assert_eq!(text, "Menu 世界");
    assert_eq!(
        component_title(&serde_json::json!({"String":"x".repeat(4000)})).len(),
        512
    );
}

#[test]
fn native_keyboard_gestures_cannot_enter_the_bedrock_inventory_transport() {
    let mut ui = UiRuntime::new(7);
    ui.publish_java_hud(hud(7, 1));
    let mut player = player_state::PlayerState::new(7);
    let hit = Some(ui_screens::container_actions::InventoryCellHit::Player(0));
    assert!(
        super::super::interaction::dispatch_inventory_hotbar(&mut player, &mut ui, hit, 1)
            .is_none()
    );
    assert!(ui.java_inventory_error().is_some());
    assert!(
        super::super::interaction::dispatch_inventory_key(
            &mut player,
            &mut ui,
            hit,
            semantic_input::KeyboardKey::KeyQ,
            false,
            None,
            true
        )
        .is_none()
    );
    assert_eq!(player.inventory.ledger().pending_request_count(), 0);
    assert!(
        ui.next_java_inventory_input().is_none(),
        "unsupported gestures do not forge a different native action either"
    );
}

#[test]
fn server_title_translates_only_component_keys_and_preserves_literal_text() {
    let lookup = |key: &str| (key == "container.chest").then(|| "Chest".into());
    assert_eq!(
        component_title_with(
            &serde_json::json!({"Compound": {
                "translate":{"String":"container.chest"},"extra":{"List":[{"String":" test"}]}
            }}),
            &lookup
        ),
        "Chest test"
    );
    assert_eq!(
        component_title_with(&serde_json::json!({"String":"container.chest"}), &lookup),
        "container.chest"
    );
    assert_eq!(
        component_title_with(
            &serde_json::json!({"Compound":{
                "translate":{"String":"server.unknown"},"fallback":{"String":"Server title"}
            }}),
            &lookup
        ),
        "Server title"
    );
}

#[test]
fn losing_native_player_clears_menu_input_and_preserves_an_unrelated_bedrock_screen() {
    let mut ui = UiRuntime::new(7);
    ui.publish_java_hud(hud(7, 1));
    ui.session.inventory_open = true;
    ui.session.java_inventory_inputs.push_back(Input {
        identity: inventory::java::pickup::MenuIdentity {
            session: 7,
            epoch: 1,
            container: 2,
            incarnation: 1,
        },
        sequence: 1,
        intent: Intent::Close,
    });
    ui.clear_java_hud();
    assert!(!ui.inventory_open());
    assert!(ui.next_java_inventory_input().is_none());
    assert!(ui.java_container_slots().is_none());
    ui.begin_session(8);
    ui.session.inventory_open = true;
    ui.clear_java_hud();
    assert!(
        ui.inventory_open(),
        "native cleanup cannot close a Bedrock-owned screen"
    );
}

#[test]
fn server_menu_replacement_retires_old_pointer_chat_and_queued_actions() {
    use inventory::java::Update;
    let mut world = client_world::java::WorldState::default();
    world.begin(7);
    world.inventory.begin(7, 1);
    let mut ui = UiRuntime::new(7);
    ui.publish_java_hud(hud(7, 1));
    ui.publish_java_inventory(&world);
    for incarnation in [1, 2] {
        ui.session.chat_focused = true;
        ui.session.screen.hover = Some(ui_screens::container_actions::InventoryCellHit::Player(0));
        ui.session.inventory_pointer_gui = Some([5.0, 5.0]);
        ui.queue_java_inventory(Intent::Close);
        assert!(ui.next_java_inventory_input().is_some());
        world
            .inventory
            .admit(
                7,
                1,
                Update::Open {
                    incarnation,
                    container: 2,
                    kind: "GENERIC_9X3".into(),
                    title: serde_json::Value::Null,
                },
            )
            .unwrap();
        ui.publish_java_inventory(&world);
        assert!(ui.inventory_open());
        assert!(!ui.session.chat_focused);
        assert!(ui.session.screen.hover.is_none());
        assert!(ui.session.inventory_pointer_gui.is_none());
        assert!(ui.next_java_inventory_input().is_none());
        assert_eq!(
            ui.session.java_inventory.identity.unwrap().incarnation,
            incarnation
        );
    }
}
