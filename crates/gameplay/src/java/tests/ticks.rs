use super::*;

#[test]
fn loading_closes_one_boundary_per_deadline_without_pose_or_catchup_movement() {
    let (mut state, physics) = super::flight::flight_world()
        .expect("tick boundary test requires the existing pinned Java physics fixture");
    let original = state.player_motion;
    let mut c = Controller::default();
    c.begin_frame(&state, TICK * 10);
    for n in 1..=10 {
        assert!(c.game_tick(&mut state, None, Controls::default()));
        assert_eq!(c.pending_tick_end(), Some(1));
        assert!(c.pending().is_none());
        assert_eq!(c.game_ticks, n);
        assert_eq!(c.ticks, 0);
        assert_eq!(state.player_motion, original);
        assert!(
            !c.game_tick(&mut state, Some(&physics), Controls::default()),
            "backpressure retains this boundary and forbids physics advancement"
        );
        c.tick_queued();
        assert!(c.pending_tick_end().is_none());
    }
    assert_eq!(c.backlog_ticks(), 0);
    assert!(!c.game_tick(&mut state, Some(&physics), Controls::default()));
    c.begin_frame(&state, TICK);
    assert!(c.game_tick(&mut state, Some(&physics), Controls::default()));
    assert!(c.pending().is_some());
    assert_eq!((c.game_ticks, c.ticks), (11, 1));
    c.tick_queued();
    assert!(c.pending().is_none() && c.pending_tick_end().is_none());
    assert!(
        !c.game_tick(&mut state, Some(&physics), Controls::default()),
        "resuming movement must not emit another idle boundary or replay loading time"
    );
}

#[test]
fn correction_keeps_the_logical_deadline_and_closes_without_ack_or_pose_forgery() {
    use java_protocol::session::{Envelope, Event, facts::Teleport};
    let mut state = world(1);
    let mut c = Controller::default();
    c.begin_frame(&state, TICK / 2);
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: Teleport {
                id: 7,
                position: [3., 64., 4.],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: 0
            }
        }
    }));
    assert!(c.begin_frame(&state, TICK / 2));
    let motion = state.player_motion;
    assert!(state.awaiting_correction_ack());
    assert!(c.game_tick(&mut state, None, Controls::default()));
    assert!(c.pending().is_none());
    assert_eq!(c.pending_tick_end(), Some(1));
    assert_eq!(state.player_motion, motion);
    assert!(
        state.awaiting_correction_ack(),
        "a boundary does not acknowledge a teleport"
    );
    c.tick_queued();
    assert!(!c.game_tick(&mut state, None, Controls::default()));
    state.correction_queued(1);
    assert!(!state.awaiting_correction_ack());
}

#[test]
fn full_queue_and_replacement_retire_only_the_correct_world_boundary() {
    let mut state = world(1);
    let mut c = Controller::default();
    c.begin_frame(&state, TICK);
    assert!(c.game_tick(&mut state, None, Controls::default()));
    c.begin_frame(&state, TICK * 3);
    assert!(!c.game_tick(&mut state, None, Controls::default()));
    assert_eq!((c.game_ticks, c.backlog_ticks()), (1, 3));
    assert_eq!(c.pending_tick_end(), Some(1));
    let replacement = world(2);
    c.begin_frame(&replacement, Duration::ZERO);
    assert!(c.pending_tick_end().is_none());
    assert_eq!((c.game_ticks, c.backlog_ticks()), (0, 0));
    assert!(!c.game_tick(&mut state, None, Controls::default()));
}

#[test]
fn connection_without_a_logical_world_has_no_play_tick_boundaries() {
    let mut state = WorldState::default();
    let mut c = Controller::default();
    c.begin_frame(&state, TICK * 20);
    assert!(!c.game_tick(&mut state, None, Controls::default()));
    assert!(c.pending_tick_end().is_none());
    assert_eq!(c.backlog_ticks(), 0);
}

#[test]
fn missing_collision_facts_close_ticks_without_predicting_a_position_or_flags() {
    let (mut state, physics) = super::flight::flight_world()
        .expect("collision-wait test requires the existing pinned Java physics fixture");
    state.player_motion.as_mut().unwrap().position = [1000., 70., 1000.];
    let before = state.player_motion;
    let mut c = Controller::default();
    c.begin_frame(&state, TICK * 2);
    for _ in 0..2 {
        assert!(c.game_tick(&mut state, Some(&physics), Controls::default()));
        assert!(c.blocked.is_some(), "missing collision remains diagnosed");
        assert!(c.pending().is_none());
        assert_eq!(state.player_motion, before);
        assert!(!c.body.on_ground, "no synthetic landing flag");
        assert_eq!(c.ticks, 0);
        c.tick_queued();
    }
    assert_eq!((c.game_ticks, c.backlog_ticks()), (2, 0));
}

#[test]
fn queue_wait_cannot_turn_into_movement_when_physics_becomes_ready() {
    let (mut state, physics) = super::flight::flight_world()
        .expect("queue-wait test requires the existing pinned Java physics fixture");
    let original = state.player_motion;
    let mut c = Controller::default();
    c.begin_frame(&state, TICK);
    assert!(c.game_tick(&mut state, None, Controls::default()));
    // The pending idle boundary cannot enter a full transport queue for three more ticks.
    c.begin_frame(&state, TICK * 3);
    assert!(!c.game_tick(&mut state, Some(&physics), Controls::default()));
    c.tick_queued();
    for _ in 0..3 {
        assert!(c.game_tick(&mut state, Some(&physics), Controls::default()));
        assert!(
            c.pending().is_none(),
            "old waiting time must not become physics"
        );
        assert_eq!(state.player_motion, original);
        c.tick_queued();
    }
    assert_eq!((c.game_ticks, c.ticks, c.backlog_ticks()), (4, 0, 0));
    assert!(!c.game_tick(&mut state, Some(&physics), Controls::default()));
    c.begin_frame(&state, TICK);
    assert!(c.game_tick(&mut state, Some(&physics), Controls::default()));
    assert!(c.pending().is_some());
    assert_eq!(c.ticks, 1);
}

#[test]
fn unsent_motion_replaced_by_corrections_retains_only_one_consumed_boundary() {
    use java_protocol::session::{Envelope, Event, facts::Teleport};
    let (mut state, physics) = super::flight::flight_world()
        .expect("correction test requires the existing pinned Java physics fixture");
    let mut c = Controller::default();
    c.begin_frame(&state, TICK * 2);
    assert!(c.game_tick(&mut state, Some(&physics), Controls::default()));
    assert!(c.pending().is_some());
    for revision in 1..=2 {
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Teleport {
                dimension_epoch: 1,
                sequence: revision,
                correction: Teleport {
                    id: 7,
                    position: [8.5, 2., 8.5],
                    velocity: [0.; 3],
                    yaw: 0.,
                    pitch: 0.,
                    relative: 0
                }
            }
        }));
        assert!(c.begin_frame(&state, Duration::ZERO));
        assert!(c.pending().is_none());
        assert_eq!(c.pending_tick_end(), Some(1));
        assert_eq!(c.game_ticks, 1);
    }
    c.tick_queued();
    let placed = state.player_motion;
    assert!(c.game_tick(&mut state, Some(&physics), Controls::default()));
    assert!(c.pending().is_none());
    assert_eq!(state.player_motion, placed);
    assert_eq!(c.game_ticks, 2);
    c.tick_queued();
    assert!(!c.game_tick(&mut state, None, Controls::default()));
}
