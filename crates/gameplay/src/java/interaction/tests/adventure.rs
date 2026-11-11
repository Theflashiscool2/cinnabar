//! Native permission checks at START and local FINISH, not on each continuing mining tick.
use super::*;
use assets::registry::java::adventure::Permission;
use std::sync::Arc;
use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
fn scene(predicate: Option<serde_json::Value>) -> WorldState {
    let mut state = mining_state();
    state.status.mode = Some(GameMode::Adventure);
    let mut held = state.inventory.held().unwrap().as_ref().unwrap().clone();
    if let Some(predicate) = predicate {
        held.added
            .insert(Permission::Break.component().into(), predicate);
    }
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(held),
            },
        )
        .unwrap();
    let section = Arc::new(Section {
        blocks: Palette::uniform(BlockStateId(1), 4096),
        biomes: Palette::uniform(BiomeId(0), 64),
    });
    state.terrain.replace(
        [0, 0],
        Column::new(
            vec![section; 24],
            Light {
                sky: vec![None; 26],
                block: vec![None; 26],
            },
            vec![],
            vec![],
        ),
    );
    state
}
fn target() -> Hit {
    Hit { state: 1, ..hit() }
}
fn start(controller: &mut Controller, state: &WorldState) {
    begin(controller, state, Duration::ZERO);
    controller.contact = Some((true, false));
    controller.advance(state, press(), Some(target()));
}
fn finish(controller: &mut Controller, state: &WorldState) {
    for _ in 0..8 {
        begin(controller, state, simulation_clock::TICK_DURATION);
        controller.advance(
            state,
            ActionPhase {
                held: true,
                ..Default::default()
            },
            Some(target()),
        );
    }
    assert_eq!(controller.pending().unwrap().kind, Kind::FinishBreak);
}
#[test]
fn adventure_requires_matching_can_break_before_start_then_uses_native_mining_progress() {
    let denied = scene(None);
    let mut controller = Controller::default();
    start(&mut controller, &denied);
    assert!(controller.pending().is_none());
    assert!(controller.blocked.as_ref().unwrap().contains("can_break"));
    let allowed = scene(Some(serde_json::json!([{"blocks":{"ids":[1]}}])));
    let mut controller = Controller::default();
    start(&mut controller, &allowed);
    assert_eq!(controller.pending().unwrap().kind, Kind::Break);
    assert!(!controller.pending_destroys_block());
    controller.queued();
    finish(&mut controller, &allowed);
    assert!(permission::check(&allowed, target(), Hand::Main, Permission::Break).unwrap());
}
#[test]
fn continuing_native_target_retains_component_cache_across_tag_rebinding() {
    let mut state = scene(Some(
        serde_json::json!([{"blocks":{"tag":"fixture:allowed"}}]),
    ));
    state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .tags
        .get_mut("minecraft:block")
        .unwrap()
        .insert("fixture:allowed".into(), vec![1]);
    let mut controller = Controller::default();
    start(&mut controller, &state);
    controller.queued();
    let mut tags = state.bootstrap.as_ref().unwrap().configuration.tags.clone();
    tags.get_mut("minecraft:block")
        .unwrap()
        .insert("fixture:allowed".into(), vec![]);
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Tags(tags)
    }));
    assert_eq!(state.tag_revision, 1);
    finish(&mut controller, &state);
    assert!(
        permission::check(&state, target(), Hand::Main, Permission::Break).unwrap(),
        "native same-state cache survives changed tag membership"
    );
}
#[test]
fn same_component_wire_replacement_rechecks_local_finish_without_cancelling_the_finish_packet() {
    let mut state = scene(Some(
        serde_json::json!([{"blocks":{"tag":"fixture:allowed"}}]),
    ));
    state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .tags
        .get_mut("minecraft:block")
        .unwrap()
        .insert("fixture:allowed".into(), vec![1]);
    let mut controller = Controller::default();
    start(&mut controller, &state);
    controller.queued();
    state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .tags
        .get_mut("minecraft:block")
        .unwrap()
        .insert("fixture:allowed".into(), vec![]);
    let held = state.inventory.held().unwrap().clone();
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: held,
            },
        )
        .unwrap();
    finish(&mut controller, &state);
    assert!(
        !permission::check(&state, target(), Hand::Main, Permission::Break).unwrap(),
        "new predicate object denies local destruction, not its unconditional FINISH packet"
    );
}
#[test]
fn granted_instabuild_is_checked_after_adventure_permission_not_inferred_from_mode() {
    let mut state = scene(Some(serde_json::json!([{"blocks":{"ids":[1]}}])));
    state.player_dynamics.abilities.as_mut().unwrap().creative = true;
    let mut controller = Controller::default();
    start(&mut controller, &state);
    assert_eq!(controller.pending().unwrap().kind, Kind::Break);
    assert!(controller.pending_destroys_block());
    let mut denied = scene(None);
    denied.player_dynamics.abilities.as_mut().unwrap().creative = true;
    let mut controller = Controller::default();
    start(&mut controller, &denied);
    assert!(
        controller.pending().is_none(),
        "instabuild does not bypass Adventure mayBuild restriction"
    );
}
