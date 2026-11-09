use super::*;
use java_protocol::session::{
    Envelope, Event,
    facts::{Teleport, Vitals},
};
use sim::java::selection::Face;
use world::java::{Dimension, dynamics::Abilities, hud::GameMode};

#[path = "tests/tags.rs"]
mod tags;

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
fn survival_use_is_buffered_and_does_not_predict_blocks_or_stack_consumption() {
    let mut state = state();
    state.status.mode = Some(GameMode::Survival);
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:dirt".into(),
                    count: 7,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.use_on(&state, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().kind, Kind::UseOn);
    assert_eq!(c.pending().unwrap().sequence, 1);
    begin(&mut c, &state, Duration::from_secs(1));
    c.use_on(&state, ActionPhase::default(), None);
    assert_eq!(c.pending().unwrap().target, Target::Block(hit()));
    c.queued();
    assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 7);
    assert_eq!(state.block_updates, 0);
    assert_eq!(state.terrain.len(), 0);
    for _ in 0..3 {
        begin(&mut c, &state, simulation_clock::TICK_DURATION);
        c.use_on(
            &state,
            ActionPhase {
                held: true,
                ..Default::default()
            },
            Some(hit()),
        );
        c.advance(&state, ActionPhase::default(), Some(hit()));
        assert!(c.pending().is_none());
    }
    begin(&mut c, &state, simulation_clock::TICK_DURATION);
    c.use_on(
        &state,
        ActionPhase {
            held: true,
            ..Default::default()
        },
        Some(hit()),
    );
    assert_eq!(c.pending().unwrap().sequence, 2);
    c.queued();
    // A server rejection/unchanged slot remains authoritative: another attempt never decrements it.
    assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 7);
}
#[test]
fn held_use_keeps_tick_remainder_without_accumulating_transport_wait() {
    let state = state();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.use_on(&state, press(), Some(hit()));
    c.queued();
    let held = ActionPhase {
        held: true,
        ..Default::default()
    };
    begin(&mut c, &state, Duration::from_millis(240));
    c.use_on(&state, held, Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 2);
    // Queuing at a frame boundary must preserve the 40ms tick remainder. Waiting for
    // transport afterward must not manufacture extra held uses when it finally drains.
    begin(&mut c, &state, Duration::from_secs(2));
    c.queued();
    begin(&mut c, &state, Duration::from_millis(159));
    c.use_on(&state, held, Some(hit()));
    assert!(c.pending().is_none());
    begin(&mut c, &state, Duration::from_millis(1));
    c.use_on(&state, held, Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 3);
}
#[test]
fn use_and_break_share_sequence_and_authority_retirement() {
    let mut state = state();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    c.queued();
    c.use_on(&state, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 2);
    c.queued();
    c.advance(&state, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 3);
    c.queued();
    c.use_on(&state, press(), Some(hit()));
    c.begin_frame(
        &state,
        Duration::ZERO,
        Some(InputAuthorityGeneration::INITIAL),
        true,
    );
    assert!(c.pending().is_none());
    begin(&mut c, &state, Duration::ZERO);
    c.use_on(&state, press(), Some(hit()));
    state.status.mode = Some(GameMode::Spectator);
    begin(&mut c, &state, Duration::ZERO);
    assert!(c.pending().is_none());
    c.use_on(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    state.status.mode = Some(GameMode::Survival);
    c.use_on(&state, press(), Some(hit()));
    state.inventory = Default::default();
    begin(&mut c, &state, Duration::ZERO);
    assert!(c.pending().is_none());
    state.begin(8);
    begin(&mut c, &state, Duration::ZERO);
    assert!(c.last_queued.is_none());
}
#[test]
fn unknown_inventory_and_creative_swords_do_not_send_breaks() {
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
    assert_eq!(c.pending().unwrap().target, Target::Block(hit()));
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
fn survival_without_required_registry_and_death_never_emit_breaks() {
    let mut state = state();
    state.status.mode = Some(GameMode::Survival);
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
    assert!(c.blocked.as_ref().unwrap().contains("registry"));
    state.status.mode = Some(GameMode::Creative);
    state.vitals.as_mut().unwrap().health = 0.;
    c.advance(&state, press(), Some(hit()));
    assert!(c.pending().is_none());
}

#[test]
fn entity_or_air_target_does_not_report_an_unsupported_survival_block_break() {
    let mut s = state();
    s.status.mode = Some(GameMode::Survival);
    let mut controller = Controller::default();
    begin(&mut controller, &s, Duration::ZERO);
    controller.advance(&s, press(), None);
    assert!(controller.blocked.is_none());
    assert!(controller.pending().is_none());
    controller.advance(&s, press(), Some(hit()));
    assert!(controller.blocked.as_deref().unwrap().contains("registry"));
}

#[test]
fn air_requests_share_sequences_and_keep_backpressure_out_of_repeat_time() {
    let state = state();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    c.use_target(&state, press(), Ok(Some(Target::Air([90., -45.]))));
    let first = c.pending().unwrap();
    assert_eq!(first.kind, Kind::UseAir);
    assert_eq!(first.target, Target::Air([90., -45.]));
    assert_eq!(first.sequence, 1);
    begin(&mut c, &state, Duration::from_secs(2));
    c.use_target(&state, press(), Ok(Some(Target::Air([0., 0.]))));
    assert_eq!(c.pending().unwrap().target, first.target);
    c.queued();
    begin(&mut c, &state, Duration::from_millis(199));
    let held = ActionPhase {
        held: true,
        ..Default::default()
    };
    c.use_target(&state, held, Ok(Some(Target::Air([30., -10.]))));
    assert!(c.pending().is_none());
    begin(&mut c, &state, Duration::from_millis(1));
    c.use_target(&state, held, Ok(Some(Target::Air([30., -10.]))));
    assert_eq!(c.pending().unwrap().sequence, 2);
    c.queued();
    c.use_target(&state, press(), Ok(None));
    assert!(c.pending().is_none());
    c.use_on(&state, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().sequence, 3);
    assert_eq!(c.pending().unwrap().target, Target::Block(hit()));
    c.begin_frame(&state, Duration::ZERO, None, true);
    assert!(c.pending().is_none());
}

fn mining_state() -> WorldState {
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    let mut s = state();
    s.status.mode = Some(GameMode::Survival);
    let blocks=br#"{"minecraft:air":{"states":[{"id":0,"default":true}]},"minecraft:stone":{"states":[{"id":1,"default":true}]}}"#;
    s.install_block_registry(Arc::new(
        assets::registry::java::BlockRegistry::load(
            blocks,
            &format!("{:x}", Sha256::digest(blocks)),
        )
        .unwrap(),
    ))
    .unwrap();
    let spawn = java_protocol::session::facts::Spawn {
        dimension_type: 0,
        world: "fixture:interaction".into(),
        hashed_seed: 0,
        game_mode: 0,
        previous_game_mode: -1,
        debug: false,
        flat: true,
        last_death: None,
        portal_cooldown: 0,
        sea_level: 63,
    };
    s.bootstrap = Some(Box::new(java_protocol::session::facts::Bootstrap {
        generation: 7,
        profile: java_protocol::auth::Profile {
            id: Default::default(),
            name: "Fixture".into(),
        },
        server_session: Default::default(),
        configuration: Default::default(),
        world: java_protocol::session::facts::World {
            entity_id: 42,
            hardcore: false,
            worlds: vec![spawn.world.clone()],
            max_players: 2,
            view_distance: 2,
            simulation_distance: 2,
            reduced_debug: false,
            respawn_screen: true,
            limited_crafting: false,
            spawn,
            online_mode: false,
            secure_chat: false,
        },
    }));
    s.bootstrap.as_mut().unwrap().configuration.tags.insert(
        "minecraft:block".into(),
        [("minecraft:mineable/pickaxe".into(), vec![1])].into(),
    );
    s.player_dynamics.attributes = java_protocol::target::PLAYER_ATTRIBUTES
        .iter()
        .map(|&(id, base)| {
            let spec = java_protocol::target::ATTRIBUTES[id];
            (
                spec.name.into(),
                world::java::dynamics::Attribute {
                    base,
                    minimum: spec.minimum,
                    maximum: spec.maximum,
                    modifiers: vec![],
                },
            )
        })
        .collect();
    s.inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:iron_pickaxe".into(),
                    count: 1,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    s
}
#[test]
fn survival_start_finish_and_abort_are_ordered_without_local_block_or_item_changes() {
    let s = mining_state();
    let mut h = hit();
    h.state = 1;
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.contact = Some((true, false));
    c.advance(&s, press(), Some(h));
    assert_eq!(c.pending().unwrap().kind, Kind::Break);
    c.queued();
    let held = ActionPhase {
        held: true,
        ..Default::default()
    };
    // Native iron speed6 / hardness1.5 / harvest divisor30 => eight continuing ticks.
    for _ in 0..7 {
        begin(&mut c, &s, simulation_clock::TICK_DURATION);
        c.advance(&s, held, Some(h));
        assert!(c.pending().is_none());
    }
    begin(&mut c, &s, simulation_clock::TICK_DURATION);
    c.advance(&s, held, Some(h));
    assert_eq!(c.pending().unwrap().kind, Kind::FinishBreak);
    assert_eq!(c.pending().unwrap().sequence, 2);
    c.queued();
    assert!(c.mining_progress().is_none());
    assert_eq!(s.block_updates, 0);
    assert_eq!(s.inventory.held().unwrap().as_ref().unwrap().count, 1);
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.contact = Some((true, false));
    c.advance(&s, press(), Some(h));
    c.queued();
    c.advance(
        &s,
        ActionPhase {
            released: true,
            ..Default::default()
        },
        Some(h),
    );
    assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
    begin(&mut c, &s, Duration::from_secs(4));
    assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
    c.queued();
    assert!(c.mining_progress().is_none());
}
#[test]
fn mining_item_changes_and_input_release_abort_but_corrections_retire_old_intent() {
    let mut s = mining_state();
    let mut h = hit();
    h.state = 1;
    let held = ActionPhase {
        held: true,
        ..Default::default()
    };
    for mode in 0..3 {
        let mut c = Controller::default();
        begin(&mut c, &s, Duration::ZERO);
        c.contact = Some((true, false));
        c.advance(&s, press(), Some(h));
        c.queued();
        match mode {
            0 => {
                s.inventory
                    .admit(
                        7,
                        1,
                        inventory::java::Update::PlayerSlot {
                            slot: 0,
                            stack: None,
                        },
                    )
                    .unwrap();
                c.advance(&s, held, Some(h));
                assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
                s = mining_state();
            }
            1 => {
                c.begin_frame(&s, Duration::ZERO, None, true);
                assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
                c.begin_frame(&s, Duration::ZERO, None, true);
                assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
            }
            _ => {
                s.begin(8);
                begin(&mut c, &s, Duration::from_secs(2));
                assert!(c.pending().is_none());
                assert!(c.mining_progress().is_none());
            }
        }
    }
}

#[test]
fn replaced_or_unloaded_targets_retire_backpressured_start_and_finish() {
    let s = mining_state();
    let mut h = hit();
    h.state = 1;
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.contact = Some((true, false));
    c.advance(&s, press(), Some(h));
    assert!(!c.validate_mining_target(None));
    assert!(c.pending().is_none());
    assert_eq!(c.queued_actions, 0, "an unsent start needs no server abort");
    c.advance(&s, press(), Some(h));
    assert!(c.validate_mining_target(Some(1)));
    c.queued();
    let held = ActionPhase {
        held: true,
        ..Default::default()
    };
    for _ in 0..8 {
        begin(&mut c, &s, simulation_clock::TICK_DURATION);
        c.advance(&s, held, Some(h));
    }
    assert_eq!(c.pending().unwrap().kind, Kind::FinishBreak);
    assert!(
        !c.validate_mining_target(Some(10)),
        "a replacement dirt block is not the old stone target"
    );
    assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
    assert!(
        c.validate_mining_target(None),
        "an abort still retires an already started action after unload"
    );
    c.queued();
    assert!(c.mining_progress().is_none());
    assert_eq!(c.last_queued.unwrap().kind, Kind::AbortBreak);
    assert_eq!(s.block_updates, 0);
}

#[test]
fn zero_speed_zero_hardness_waits_for_release_instead_of_repeating_instant_starts() {
    use sha2::{Digest, Sha256};
    let mut s = mining_state();
    // Independently pinned native ID42 has zero hardness. The fixture names are
    // deliberately outside all tool rules, so speed1 * server attribute0 = 0/0.
    let report: serde_json::Map<String, serde_json::Value> = (0..=42)
        .map(|id| {
            (
                format!("minecraft:fixture_state_{id}"),
                serde_json::json!({"states":[{"id":id,"default":true}]}),
            )
        })
        .collect();
    let bytes = serde_json::to_vec(&report).unwrap();
    s.install_block_registry(std::sync::Arc::new(
        assets::registry::java::BlockRegistry::load(
            &bytes,
            &format!("{:x}", Sha256::digest(&bytes)),
        )
        .unwrap(),
    ))
    .unwrap();
    assert_eq!(
        assets::registry::java::mining::builtin()
            .unwrap()
            .state(42)
            .unwrap()
            .hardness,
        0.
    );
    s.player_dynamics
        .attributes
        .get_mut("minecraft:block_break_speed")
        .unwrap()
        .base = 0.;
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.contact = Some((true, false));
    c.advance(&s, press(), Some(hit()));
    assert_eq!(c.pending().unwrap().kind, Kind::Break);
    assert!(
        c.mining_progress().is_some(),
        "native >=1 is false for NaN; this is not an instant action"
    );
    c.queued();
    for _ in 0..4 {
        begin(&mut c, &s, simulation_clock::TICK_DURATION);
        c.advance(
            &s,
            ActionPhase {
                held: true,
                ..Default::default()
            },
            Some(hit()),
        );
        assert!(c.pending().is_none());
    }
    assert!(c.mining_progress().unwrap().1.is_nan());
    c.advance(
        &s,
        ActionPhase {
            released: true,
            ..Default::default()
        },
        Some(hit()),
    );
    assert_eq!(c.pending().unwrap().kind, Kind::AbortBreak);
    assert_eq!(s.block_updates, 0);
}

#[test]
fn native_cancel_does_not_allocate_another_prediction_sequence() {
    let s = mining_state();
    let mut h = hit();
    h.state = 1;
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.contact = Some((true, false));
    c.advance(&s, press(), Some(h));
    assert_eq!(c.pending().unwrap().sequence, 1);
    c.queued();
    c.advance(
        &s,
        ActionPhase {
            released: true,
            ..Default::default()
        },
        Some(h),
    );
    assert_eq!(
        (c.pending().unwrap().kind, c.pending().unwrap().sequence),
        (Kind::AbortBreak, 0)
    );
    c.queued();
    c.advance(&s, press(), Some(h));
    assert_eq!(c.pending().unwrap().sequence, 2);
}

#[test]
fn active_use_waits_for_transport_release_is_unpredicted_and_never_consumes() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    let mut state = state();
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:apple".into(),
                    count: 3,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut c = Controller::default();
    begin(&mut c, &state, Duration::ZERO);
    let plan = Plan {
        send: true,
        result: ResultKind::Consume(Some(Active {
            ticks: 32,
            end: End::Complete,
            effects: Effects {
                can_sprint: false,
                speed_multiplier: 0.2,
            },
        })),
    };
    c.use_target(&state, press(), Ok(Some(Target::Air([10., 0.]))));
    c.prepare_use(&state, plan);
    assert!(!c.using_item());
    c.queued();
    assert!(c.using_item());
    c.maintain_use(&state, 8, false);
    assert_eq!(c.use_ticks(), Some(8));
    c.maintain_use(&state, 8, true);
    assert_eq!(c.pending().unwrap().kind, Kind::ReleaseUse);
    assert_eq!(c.pending().unwrap().sequence, 0);
    assert!(c.using_item(), "backpressure retains native slowdown");
    c.queued();
    assert!(!c.using_item());
    assert_eq!(state.inventory.held().unwrap().as_ref().unwrap().count, 3);
    c.use_target(&state, press(), Ok(Some(Target::Air([0., 0.]))));
    c.prepare_use(&state, plan);
    c.queued();
    c.maintain_use(&state, 40, false);
    assert!(
        c.using_item(),
        "local duration is not a server completion acknowledgement"
    );
    state.player_visual.complete_use().unwrap();
    c.maintain_use(&state, 41, false);
    assert!(!c.using_item());
    assert!(
        c.pending().is_none(),
        "server completion invents no release/ack/stack mutation"
    );
}

#[test]
fn guaranteed_use_on_fallback_is_bounded_ordered_and_retired_on_world_change() {
    use crate::java::item_use::plan::{Plan, ResultKind};
    let mut s = state();
    s.inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:apple".into(),
                    count: 3,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.use_on(&s, press(), Some(hit()));
    c.prepare_use_fallback(
        &s,
        [91., 19.],
        Plan {
            send: true,
            result: ResultKind::Pass,
        },
    );
    assert_eq!(c.pending().unwrap().kind, Kind::UseOn);
    c.queued();
    let air = c.pending().unwrap();
    assert_eq!(air.kind, Kind::UseAir);
    assert_eq!(air.sequence, 2);
    assert_eq!(air.target, Target::Air([91., 19.]));
    c.queued();
    assert!(c.pending().is_none());
    assert!(!c.using_item());
    c.use_on(&s, press(), Some(hit()));
    c.prepare_use_fallback(
        &s,
        [91., 19.],
        Plan {
            send: true,
            result: ResultKind::Pass,
        },
    );
    s.terrain.begin(Dimension {
        session: 7,
        epoch: 2,
        name: "fixture:replacement".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    begin(&mut c, &s, Duration::ZERO);
    assert!(c.pending().is_none());
    c.queued();
    assert!(c.pending().is_none());
}

#[test]
fn active_use_cannot_bank_mining_ticks_or_a_burst_of_following_uses() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    let mut s = state();
    s.inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:apple".into(),
                    count: 3,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.use_target(&s, press(), Ok(Some(Target::Air([0., 0.]))));
    c.prepare_use(
        &s,
        Plan {
            send: true,
            result: ResultKind::Consume(Some(Active {
                ticks: 1200,
                end: End::Complete,
                effects: Effects {
                    can_sprint: false,
                    speed_multiplier: 0.2,
                },
            })),
        },
    );
    c.queued();
    begin(&mut c, &s, Duration::from_secs(20));
    assert_eq!(c.backlog_ticks(), 0);
    assert_eq!(c.use_elapsed, simulation_clock::TICK_DURATION * 4);
    c.advance(&s, press(), Some(hit()));
    assert!(c.pending().is_none());
    assert!(c.mining_progress().is_none());
    assert_eq!(c.backlog_ticks(), 0);
}

#[test]
fn old_stop_does_not_cancel_a_new_use_but_a_new_authoritative_stop_does() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    let mut s = state();
    s.inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:apple".into(),
                    count: 2,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    s.player_visual.observe_use(0).unwrap();
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    let plan = Plan {
        send: true,
        result: ResultKind::Consume(Some(Active {
            ticks: 32,
            end: End::Complete,
            effects: Effects {
                can_sprint: false,
                speed_multiplier: 0.2,
            },
        })),
    };
    c.use_target(&s, press(), Ok(Some(Target::Air([0., 0.]))));
    c.prepare_use(&s, plan);
    c.queued();
    c.maintain_use(&s, 100, false);
    assert!(
        c.using_item(),
        "old stop and local duration are not new authority"
    );
    s.player_visual.observe_use(0).unwrap();
    c.maintain_use(&s, 101, false);
    assert!(!c.using_item());
    assert!(c.pending().is_none());
    s.player_visual.observe_use(1).unwrap();
    c.maintain_use(&s, 102, false);
    assert_eq!(c.server_use_requested(), Some(super::Hand::Main));
    c.resume_server_use(&s, plan);
    assert!(c.using_item());
    assert!(
        c.pending().is_none(),
        "server start does not emit another UseItem"
    );
    s.player_visual.complete_use().unwrap();
    c.maintain_use(&s, 103, false);
    assert!(!c.using_item());
}

#[test]
fn native_four_stage_hand_order_is_captured_bounded_and_active_offhand_survives_main_selection() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    let mut s = state();
    let stack = |name: &str| {
        Some(inventory::java::Stack {
            item: name.into(),
            count: 1,
            added: Default::default(),
            removed: Default::default(),
        })
    };
    for (slot, item) in [
        (0, stack("minecraft:iron_sword")),
        (1, stack("minecraft:stone")),
        (40, stack("minecraft:shield")),
    ] {
        s.inventory
            .admit(
                7,
                1,
                inventory::java::Update::PlayerSlot { slot, stack: item },
            )
            .unwrap();
    }
    let inventory = serde_json::to_value(&s.inventory).unwrap();
    let pass = Plan {
        send: true,
        result: ResultKind::Pass,
    };
    let block = Plan {
        send: true,
        result: ResultKind::Consume(Some(Active {
            ticks: 72000,
            end: End::Complete,
            effects: Effects {
                can_sprint: false,
                speed_multiplier: 0.2,
            },
        })),
    };
    let steps = vec![
        UseStep::block(hit(), Hand::Main),
        UseStep::air(
            [30., -10.],
            Hand::Main,
            pass,
            Hand::Main.held(&s.inventory).unwrap(),
        )
        .unwrap(),
        UseStep::block(hit(), Hand::Off),
        UseStep::air(
            [30., -10.],
            Hand::Off,
            block,
            Hand::Off.held(&s.inventory).unwrap(),
        )
        .unwrap(),
    ];
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.use_chain(&s, press(), Ok(steps));
    for (index, (hand, kind)) in [
        (Hand::Main, Kind::UseOn),
        (Hand::Main, Kind::UseAir),
        (Hand::Off, Kind::UseOn),
        (Hand::Off, Kind::UseAir),
    ]
    .into_iter()
    .enumerate()
    {
        let r = c.pending().unwrap();
        assert_eq!((r.hand, r.kind, r.sequence), (hand, kind, index as i32 + 1));
        if kind == Kind::UseAir {
            assert_eq!(r.target, Target::Air([30., -10.]));
        }
        begin(&mut c, &s, Duration::from_secs(1));
        assert_eq!(
            c.pending().unwrap().sequence,
            r.sequence,
            "transport wait does not replan or advance sequences"
        );
        c.queued();
    }
    assert_eq!(c.using_hand(), Some(Hand::Off));
    assert_eq!(c.use_ticks(), Some(0));
    assert!(c.pending().is_none());
    assert_eq!(
        serde_json::to_value(&s.inventory).unwrap(),
        inventory,
        "sending use awards no item or equipment mutation"
    );
    s.inventory
        .admit(7, 1, inventory::java::Update::Selected(1))
        .unwrap();
    c.prepare_selection(&s, 12);
    assert!(
        c.pending().is_none(),
        "selection emits no release for offhand"
    );
    assert_eq!(
        c.using_hand(),
        Some(Hand::Off),
        "main selection must not cancel offhand"
    );
    c.maintain_use(&s, 12, true);
    assert_eq!(c.pending().unwrap().kind, Kind::ReleaseUse);
    assert_eq!(c.pending().unwrap().sequence, 0);
    assert!(c.using_item());
    c.queued();
    assert!(!c.using_item());
}

#[test]
fn ordered_hand_chain_retires_on_authority_change_and_rejects_terminal_or_reversed_followups() {
    use crate::java::item_use::plan::{Plan, ResultKind};
    let s = state();
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    c.use_chain(
        &s,
        press(),
        Ok(vec![
            UseStep::block(hit(), Hand::Off),
            UseStep::block(hit(), Hand::Main),
        ]),
    );
    assert!(c.pending().is_none());
    assert!(c.blocked.as_ref().unwrap().contains("ordering"));
    let empty = None;
    let terminal = UseStep::air(
        [0.; 2],
        Hand::Main,
        Plan {
            send: true,
            result: ResultKind::Unpredicted,
        },
        &empty,
    )
    .unwrap();
    c.use_chain(
        &s,
        press(),
        Ok(vec![terminal, UseStep::block(hit(), Hand::Off)]),
    );
    assert!(c.pending().is_none());
    c.use_chain(
        &s,
        press(),
        Ok(vec![
            UseStep::block(hit(), Hand::Main),
            UseStep::block(hit(), Hand::Off),
        ]),
    );
    assert_eq!(c.pending().unwrap().sequence, 1);
    c.queued();
    assert_eq!(c.pending().unwrap().hand, Hand::Off);
    c.begin_frame(
        &s,
        Duration::ZERO,
        Some(InputAuthorityGeneration::INITIAL),
        true,
    );
    assert!(c.pending().is_none());
    assert!(c.use_steps.is_empty());
}

#[test]
fn authoritative_offhand_start_resume_stop_and_item_replacement_are_not_mainhand_aliases() {
    use crate::java::item_use::plan::{Active, Effects, End, Plan, ResultKind};
    let mut s = state();
    s.inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 40,
                stack: Some(inventory::java::Stack {
                    item: "minecraft:apple".into(),
                    count: 2,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    s.player_visual.observe_use(3).unwrap();
    c.maintain_use(&s, 10, false);
    assert_eq!(c.server_use_requested(), Some(Hand::Off));
    assert!(c.using_item());
    c.resume_server_use(
        &s,
        Plan {
            send: true,
            result: ResultKind::Consume(Some(Active {
                ticks: 32,
                end: End::Complete,
                effects: Effects {
                    can_sprint: false,
                    speed_multiplier: 0.2,
                },
            })),
        },
    );
    assert_eq!(c.using_hand(), Some(Hand::Off));
    c.maintain_use(&s, 12, false);
    assert_eq!(c.use_ticks(), Some(2));
    s.player_visual.complete_use().unwrap();
    c.maintain_use(&s, 13, false);
    assert!(!c.using_item());
    assert!(c.pending().is_none());
    assert_eq!(
        Hand::Off
            .held(&s.inventory)
            .unwrap()
            .as_ref()
            .unwrap()
            .count,
        2
    );
}

#[test]
fn selected_slot_change_still_orders_mainhand_release_ahead_of_selection() {
    let mut s = state();
    let mut c = Controller::default();
    begin(&mut c, &s, Duration::ZERO);
    s.player_visual.observe_use(1).unwrap();
    c.prepare_selection(&s, 1);
    assert_eq!(c.using_hand(), Some(Hand::Main));
    assert_eq!(c.pending().unwrap().kind, Kind::ReleaseUse);
    assert_eq!(c.pending().unwrap().sequence, 0);
    c.queued();
    assert!(!c.using_item());
}

mod entity_chain;
