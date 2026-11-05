//! Native worker ownership must not retire an atlas subsequently installed by Bedrock.
use assets::registry::java::{
    items::{Catalog, HeadTextures},
    source::Jar,
};
use client_ui::ui_runtime::{UiRuntime, presentation::SessionIcons};
use std::sync::Arc;
fn fixture() -> Option<Arc<Catalog>> {
    let Some(root) = std::env::var_os("CINNABAR_JAVA_ASSETS") else {
        eprintln!("missing fixture: CINNABAR_JAVA_ASSETS with pinned client.jar");
        return None;
    };
    let target: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../java-protocol/data/target.json")).unwrap();
    let mut source = Jar::open(
        &std::path::Path::new(&root).join("client.jar"),
        target["client_assets"]["sha256"].as_str().unwrap(),
        target["client_assets"]["bytes"].as_u64().unwrap(),
    )
    .unwrap();
    Some(Arc::new(
        Catalog::load(&mut source, &["minecraft:air"]).unwrap(),
    ))
}
fn install(catalog: Arc<Catalog>) -> UiRuntime {
    let mut ui = UiRuntime::new(7);
    let mut world = client_world::java::WorldState::default();
    world.begin(7);
    world.inventory.begin(7, 1);
    world
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::Content {
                container: 0,
                state: 1,
                slots: vec![None; inventory::java::PLAYER_MENU_SIZE],
                carried: None,
            },
        )
        .unwrap();
    assert!(ui.publish_java_hud(world::java::hud::Snapshot {
        session: 7,
        epoch: 1,
        mode: None,
        hardcore: false,
        selected_slot: Some(0),
        health: None,
        maximum_health: None,
        food: None,
        saturation: None,
        armor: None,
        experience: None
    }));
    assert!(ui.publish_java_inventory(&world));
    test_time::eventually("matching native artwork publication", || {
        assert!(ui.publish_java_item_catalog(
            7,
            1,
            catalog.clone(),
            &world,
            Arc::new(HeadTextures::default())
        ));
        ui.session_icons().is_some()
    });
    ui
}
#[test]
fn native_cleanup_retires_own_atlas_but_preserves_a_new_edition_owner() {
    let Some(catalog) = fixture() else {
        return;
    };
    let mut native = install(catalog.clone());
    native.clear_java_hud();
    assert!(native.session_icons().is_none());
    let mut switched = install(catalog);
    let bedrock = Arc::new(SessionIcons::default());
    assert!(!Arc::ptr_eq(switched.session_icons().unwrap(), &bedrock));
    switched.set_session_icons(Some(bedrock.clone()));
    switched.clear_java_hud();
    assert!(Arc::ptr_eq(switched.session_icons().unwrap(), &bedrock));
    switched.clear_java_hud();
    assert!(Arc::ptr_eq(switched.session_icons().unwrap(), &bedrock));
}

#[test]
fn outside_cursor_input_is_scoped_and_never_mutates_items() {
    use client_ui::ui_runtime::java_inventory::Intent;
    use inventory::java::{Stack, Update, pickup::Button};
    let mut state = client_world::java::WorldState::default();
    state.begin(7);
    state.inventory.begin(7, 1);
    state
        .inventory
        .admit(
            7,
            1,
            Update::Open {
                incarnation: 1,
                container: 2,
                kind: "GENERIC_9X3".into(),
                title: serde_json::Value::Null,
            },
        )
        .unwrap();
    state
        .inventory
        .admit(
            7,
            1,
            Update::Content {
                container: 2,
                state: 19,
                slots: vec![None; 63],
                carried: Some(Stack {
                    item: "minecraft:emerald".into(),
                    count: 8,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut ui = UiRuntime::new(7);
    assert!(ui.publish_java_hud(world::java::hud::Snapshot {
        session: 7,
        epoch: 1,
        mode: None,
        hardcore: false,
        selected_slot: Some(0),
        health: None,
        maximum_health: None,
        food: None,
        saturation: None,
        armor: None,
        experience: None
    }));
    assert!(ui.publish_java_inventory(&state));
    let before = serde_json::to_value(&state.inventory).unwrap();
    ui.java_drop_cursor(Button::Right);
    let input = ui.next_java_inventory_input().unwrap();
    assert_eq!(input.identity, state.inventory.active_identity().unwrap());
    assert!(matches!(
        input.intent,
        Intent::DropCursor {
            button: Button::Right
        }
    ));
    assert_eq!(serde_json::to_value(&state.inventory).unwrap(), before);
    ui.clear_java_hud();
    assert!(ui.next_java_inventory_input().is_none());
    ui.java_drop_cursor(Button::Left);
    assert!(ui.next_java_inventory_input().is_none());
}
