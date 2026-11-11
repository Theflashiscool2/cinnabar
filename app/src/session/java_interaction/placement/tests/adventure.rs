//! Native permission denial is PASS after the block target packet, never a forged placement.
use super::*;
use assets::registry::java::adventure::Permission;
use gameplay::java::{
    actions,
    interaction::{Controller, Kind},
    item_use::plan::Hand,
};
use inventory::java::Update;
use world::java::{BlockStateId, dynamics::Abilities, hud::GameMode};
fn scene() -> (WorldState, Table, Request) {
    let (mut state, physics, request) = fixture();
    state.status.mode = Some(GameMode::Adventure);
    state.player_dynamics.abilities = Some(Abilities {
        invulnerable: false,
        flying: false,
        may_fly: false,
        creative: false,
        flying_speed: 0.05,
        walking_speed: 0.1,
    });
    state.vitals = Some(java_protocol::session::facts::Vitals {
        health: 20.,
        food: 20,
        saturation: 0.,
    });
    state
        .inventory
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 40,
                stack: None,
            },
        )
        .unwrap();
    use java_protocol::session::facts;
    state.bootstrap = Some(Box::new(facts::Bootstrap {
        generation: 7,
        profile: java_protocol::auth::Profile {
            id: Default::default(),
            name: "Fixture".into(),
        },
        server_session: Default::default(),
        configuration: Default::default(),
        world: facts::World {
            entity_id: 42,
            hardcore: false,
            worlds: vec!["minecraft:overworld".into()],
            max_players: 2,
            view_distance: 2,
            simulation_distance: 2,
            reduced_debug: false,
            respawn_screen: true,
            limited_crafting: false,
            online_mode: false,
            secure_chat: false,
            spawn: facts::Spawn {
                dimension_type: 0,
                world: "minecraft:overworld".into(),
                hashed_seed: 0,
                game_mode: 2,
                previous_game_mode: -1,
                debug: false,
                flat: true,
                last_death: None,
                portal_cooldown: 0,
                sea_level: 63,
            },
        },
    }));
    (state, physics, request)
}
fn requests(state: &WorldState, request: &Request) -> Vec<(Kind, Hand)> {
    let Target::Block(hit) = request.target else {
        unreachable!()
    };
    let steps = super::super::super::item::steps(
        state,
        Some(actions::Target::Block {
            hit,
            secondary: false,
            rotation: Some([0.; 2]),
        }),
    )
    .unwrap();
    let mut controller = Controller::default();
    controller.begin_frame(
        state,
        std::time::Duration::ZERO,
        Some(semantic_input::identity::InputAuthorityGeneration::INITIAL),
        false,
    );
    controller.use_chain(
        state,
        semantic_input::ActionPhase {
            pressed: true,
            held: true,
            released: false,
        },
        Ok(steps),
    );
    let mut requests = Vec::new();
    while let Some(request) = controller.pending() {
        requests.push((request.kind, request.hand));
        controller.queued();
    }
    requests
}
#[test]
fn denied_adventure_placement_still_sends_target_then_air_pass_and_offhand_target() {
    for item in ["minecraft:white_wool", "minecraft:fire_charge"] {
        let (mut state, physics, request) = scene();
        let mut held = state.inventory.held().unwrap().as_ref().unwrap().clone();
        held.item = item.into();
        state
            .inventory
            .admit(
                7,
                1,
                Update::PlayerSlot {
                    slot: 0,
                    stack: Some(held.clone()),
                },
            )
            .unwrap();
        let context = Some(Context {
            physics: &physics,
            body: Body::default(),
        });
        assert!(!super::super::super::prediction::apply(
            &mut state, &request, false, context
        ));
        assert_eq!(
            requests(&state, &request),
            vec![
                (Kind::UseOn, Hand::Main),
                (Kind::UseAir, Hand::Main),
                (Kind::UseOn, Hand::Off)
            ]
        );
        assert_eq!(state.inventory.held(), Some(&Some(held)));
        assert_eq!(state.terrain.block([-16, -59, 1]), Some(BlockStateId(0)));
        assert_eq!(
            state.entities.iter().count(),
            0,
            "local fire-charge PASS is no fireball acceptance"
        );
    }
}
#[test]
fn matching_can_place_on_predicts_consumption_but_does_not_grant_can_break() {
    let (mut state, physics, mut request) = scene();
    let mut held = state.inventory.held().unwrap().as_ref().unwrap().clone();
    held.added.insert(
        Permission::PlaceOn.component().into(),
        serde_json::json!([{"blocks":{"ids":[1]}}]),
    );
    state
        .inventory
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(held),
            },
        )
        .unwrap();
    assert_eq!(requests(&state, &request), vec![(Kind::UseOn, Hand::Main)]);
    let context = Some(Context {
        physics: &physics,
        body: Body::default(),
    });
    assert!(super::super::super::prediction::apply(
        &mut state, &request, false, context
    ));
    assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 3);
    request.sequence = 2;
    request.kind = Kind::FinishBreak;
    assert!(!super::super::super::prediction::apply(
        &mut state, &request, true, context
    ));
    assert_eq!(state.terrain.block([-16, -60, 1]), Some(BlockStateId(1)));
    let mut held = state.inventory.held().unwrap().as_ref().unwrap().clone();
    held.added.insert(
        Permission::Break.component().into(),
        serde_json::json!([{"blocks":{"ids":[1]}}]),
    );
    state
        .inventory
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(held),
            },
        )
        .unwrap();
    assert!(super::super::super::prediction::apply(
        &mut state, &request, true, context
    ));
    assert_eq!(state.terrain.block([-16, -60, 1]), Some(BlockStateId(0)));
    assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 3);
}
#[test]
fn unavailable_local_permission_retains_the_unconditional_target_request() {
    let (mut state, _, request) = scene();
    state.bootstrap = None;
    assert_eq!(requests(&state, &request), vec![(Kind::UseOn, Hand::Main)]);
}
