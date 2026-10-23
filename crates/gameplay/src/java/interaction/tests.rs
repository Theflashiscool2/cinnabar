use super::*;
use java_protocol::session::{
    Envelope, Event,
    facts::{Teleport, Vitals},
};
use sim::java::selection::Face;
use world::java::{Dimension, dynamics::Abilities, hud::GameMode};

fn state() -> WorldState {
    let mut state = WorldState::default();
    state.begin(7);
    state.terrain.begin(Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:interaction".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    state.status.mode = Some(GameMode::Creative);
    state.player_dynamics.abilities = Some(Abilities {
        creative: true,
        invulnerable: true,
        flying: false,
        may_fly: true,
        flying_speed: 0.05,
        walking_speed: 0.1,
    });
    state.vitals = Some(Vitals {
        health: 20.,
        food: 20,
        saturation: 5.,
    });
    state.inventory.begin(7, 1);
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: None,
            },
        )
        .unwrap();
    state
}
fn hit() -> Hit {
    Hit {
        block: [1, -60, 2],
        state: 42,
        position: [1.5, -59., 2.5],
        face: Face::Up,
        inside: false,
    }
}
fn begin(controller: &mut Controller, state: &WorldState, elapsed: Duration) {
    controller.begin_frame(
        state,
        elapsed,
        Some(InputAuthorityGeneration::INITIAL),
        false,
    );
}
fn press() -> ActionPhase {
    ActionPhase {
        pressed: true,
        held: true,
        released: false,
    }
}
#[test]
fn unknown_or_occupied_hands_cannot_send_an_empty_hand_creative_action() {
    let mut state = state();
    state.inventory = Default::default();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    state.inventory.begin(7, 1);
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:iron_sword".into(),
                    count: 1,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: None,
            },
        )
        .unwrap();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_some());
    state.inventory = Default::default();
    begin(&mut c, &state, Duration::ZERO);
    assert!(
        c.pending().is_none(),
        "unconfirmed inventory cancels backpressured intent"
    );
}
#[test]
fn taps_survive_backpressure_and_creative_repeat_uses_native_tick_delay() {
    let state = state();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 1);
    assert_eq!(c.queued_actions, 0);
    begin(&mut c, &state, Duration::from_secs(1));
    c.advance(&state, ActionPhase::default(), None);
    assert_eq!(c.pending().unwrap().hit, hit());
    c.queued();
    assert_eq!(c.queued_actions, 1);
    // Backpressured time cannot become a burst of future held actions.
    assert_eq!(c.backlog_ticks(), 0);
    for _ in 0..5 {
        begin(&mut c, &state, simulation_clock::TICK_DURATION);
        c.advance(
            &state,
            ActionPhase {
                held: true,
                ..Default::default()
            },
            Some(hit()),
        );
        assert!(c.pending().is_none());
    }
    begin(&mut c, &state, simulation_clock::TICK_DURATION);
    c.advance(
        &state,
        ActionPhase {
            held: true,
            ..Default::default()
        },
        Some(hit()),
    );
    assert_eq!(c.pending().unwrap().sequence, 2);
    assert_eq!(state.block_updates, 0);
    assert_eq!(state.terrain.len(), 0);
}
#[test]
fn correction_input_release_and_replacement_cancel_unsent_actions() {
    let mut state = state();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: Teleport {
                id: 1,
                position: [5., 0., 0.],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: 0
            }
        }
    }));
    begin(&mut c, &state, Duration::ZERO);
    assert!(c.pending().is_none());
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    state.correction_queued(1);
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 2);
    assert_eq!(c.pending().unwrap().correction_sequence, 1);
    c.begin_frame(
        &state,
        Duration::ZERO,
        Some(InputAuthorityGeneration::INITIAL),
        true,
    );
    assert!(c.pending().is_none());
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    state.begin(8);
    begin(&mut c, &state, Duration::ZERO);
    assert!(c.pending().is_none());
    assert!(c.last_queued.is_none());
    assert_eq!(c.queued_actions, 0);
}
#[test]
fn survival_and_death_do_not_emit_creative_breaks() {
    let mut state = state();
    state.status.mode = Some(GameMode::Survival);
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    assert!(c.blocked.as_ref().unwrap().contains("survival"));
    state.status.mode = Some(GameMode::Creative);
    state.vitals.as_mut().unwrap().health = 0.;
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
}
