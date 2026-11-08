use super::*;

fn world(epoch: u64) -> WorldState {
    let mut state = WorldState::default();
    state.begin(7);
    state.terrain.begin(world::java::Dimension {
        session: 7,
        epoch,
        name: "fixture:movement".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    state
}
fn frame() -> Frame {
    Frame {
        dimension_epoch: 1,
        sequence: 0,
        entity: 42,
        position: Some([1., 64., 2.]),
        rotation: Some([90., 25.]),
        on_ground: true,
        status_changed: true,
        horizontal_collision: false,
        input: Some(65),
        sprinting: Some(true),
    }
}

#[test]
fn queued_baselines_and_retired_dimensions_cannot_reuse_an_unsent_frame() {
    let mut controller = Controller::default();
    let state = world(1);
    controller.begin_frame(&state, TICK);
    controller.pending = Some(frame());
    assert!(controller.last_position.is_none());
    assert_eq!(controller.pending().unwrap().input, Some(65));
    controller.begin_frame(&state, TICK);
    assert_eq!(controller.pending().unwrap().position, Some([1., 64., 2.]));
    assert!(controller.last_position.is_none()); // Backpressure has not committed a send.
    controller.queued();
    assert_eq!(controller.last_position, Some([1., 64., 2.]));
    assert_eq!(controller.last_rotation, Some([90., 25.]));
    assert_eq!(controller.last_input, Some(65));
    assert_eq!(controller.last_sprint, Some(true));
    controller.pending = Some(frame());
    controller.body.water_height = 3.;
    controller.body.lava_height = 2.;
    controller.begin_frame(&world(2), TICK);
    assert!(controller.pending().is_none());
    assert!(controller.last_position.is_none());
    assert!(!controller.body.on_ground);
    assert_eq!(controller.body.water_height, 0.);
    assert_eq!(controller.body.lava_height, 0.);
}

#[test]
fn session_replacement_invalidates_unsent_prediction_without_replacing_motion() {
    let mut controller = Controller::default();
    let mut state = world(1);
    controller.begin_frame(&state, TICK);
    controller.pending = Some(frame());
    controller.body.low_profile = true;
    controller.body.swimming = true;
    // Session replacement retires the pending command without mutating new motion authority.
    state.begin(8);
    state.player_motion = Some(world::java::player::Motion {
        position: [9., 65., 8.],
        ..Default::default()
    });
    controller.begin_frame(&state, TICK);
    assert!(controller.pending().is_none());
    assert_eq!(state.player_motion.unwrap().position, [9., 65., 8.]);
    assert_eq!(controller.ticks, 0);
    assert!(!controller.body.low_profile && !controller.body.swimming);
    assert_eq!(controller.eye_height(&state), f64::from(1.62_f32));
}

#[test]
fn admitted_correction_replaces_stale_view_even_after_ack_has_already_queued() {
    use java_protocol::session::{Envelope, Event, facts::Teleport};
    let mut controller = Controller::default();
    let mut state = world(1);
    assert!(!controller.begin_frame(&state, TICK));
    controller.pending = Some(frame());
    controller.queued();
    controller.pending = Some(frame());
    controller.body.swimming = true;
    controller.body.low_profile = true;
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: Teleport {
                id: 1,
                position: [3., 64., 4.],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 45.,
                relative: 0,
            },
        },
    }));
    state.correction_queued(1);
    assert!(!state.awaiting_correction_ack());
    // The native position handler retains the current pose, even before the next tick.
    assert_eq!(controller.eye_height(&state), f64::from(0.4_f32));
    // Camera publication has not run yet: its previous 90/25 angles are stale even
    // though the network acknowledgement has completed. The caller must replace them.
    assert!(controller.begin_frame(&state, TICK));
    assert_eq!(state.player_motion.unwrap().pitch, 45.);
    assert!(controller.pending().is_none());
    assert!(controller.last_position.is_none());
    assert!(controller.last_input.is_none());
    assert!(controller.last_sprint.is_none());
    assert!(controller.last_flags.is_none());
    assert!(controller.body.swimming && controller.body.low_profile);
    assert_eq!(controller.eye_height(&state), f64::from(0.4_f32));
    assert!(!controller.begin_frame(&state, TICK));
}

#[test]
fn stationary_native_ticks_keep_tick_cadence_without_repeating_unchanged_wire_flags() {
    let state = world(1);
    let mut c = Controller::default();
    c.begin_frame(&state, TICK);
    let motion = Motion {
        position: [1., 64., 2.],
        yaw: 90.,
        pitch: 25.,
        ..Default::default()
    };
    let mut body = Body::default();
    body.on_ground = true;
    let first = c.next_frame(1, 42, motion, body, 0);
    assert!(first.position.is_some() && first.rotation.is_some() && first.status_changed);
    c.pending = Some(first);
    c.queued();
    for _ in 1..20 {
        let next = c.next_frame(1, 42, motion, body, 0);
        assert!(next.position.is_none() && next.rotation.is_none() && !next.status_changed);
        c.reminder += 1;
        c.pending = Some(next);
        c.queued();
    }
    let reminder = c.next_frame(1, 42, motion, body, 0);
    assert_eq!(reminder.position, Some([1., 64., 2.]));
    assert!(!reminder.status_changed);
    let mut airborne = body;
    airborne.on_ground = false;
    let changed = c.next_frame(1, 42, motion, airborne, 0);
    assert!(changed.status_changed);
    // Until admission, backpressure retains the original ground baseline.
    assert!(!c.next_frame(1, 42, motion, body, 0).status_changed);
    c.pending = Some(changed);
    c.queued();
    assert!(!c.next_frame(1, 42, motion, airborne, 0).status_changed);
    let mut collided = airborne;
    collided.horizontal_collision = true;
    assert!(c.next_frame(1, 42, motion, collided, 0).status_changed);
}

#[test]
fn sprint_tap_history_is_not_advanced_by_render_frames_and_retires_with_the_world() {
    let mut controller = Controller::default();
    controller.begin_frame(&world(1), TICK);
    let forward = sim::java::travel::Input {
        forward: 1.,
        ..Default::default()
    };
    let idle = sim::java::travel::Input::default();
    assert!(
        !controller
            .sprint
            .select(forward, false, true, true, false, false)
    );
    for _ in 0..30 {
        controller.begin_frame(&world(1), std::time::Duration::ZERO);
    }
    assert!(
        !controller
            .sprint
            .select(idle, false, true, true, false, false)
    );
    assert!(
        controller
            .sprint
            .select(forward, false, true, true, false, false)
    );
    controller.release_input();
    assert!(
        !controller
            .sprint
            .select(forward, true, true, true, false, false)
    );
    controller.begin_frame(&world(2), TICK);
    assert!(
        !controller
            .sprint
            .select(forward, false, true, true, false, false)
    );
}

#[test]
fn native_camera_history_is_read_only_and_retires_on_world_change() {
    let mut state = world(1);
    let mut controller = Controller::default();
    controller.begin_frame(&state, Duration::ZERO);
    let old = view::Sample {
        feet: [0., 5., 0.],
        eye_height: 1.62,
    };
    let new = view::Sample {
        feet: [0., 5., 0.2],
        eye_height: 1.27,
    };
    controller.view.reset(old);
    controller.view.tick(old, new);
    controller.elapsed = TICK / 2;
    let motion_before = state.player_motion;
    assert!((controller.view_sample(&state).unwrap().feet[2] - 0.1).abs() < 1e-10);
    assert_eq!(state.player_motion, motion_before);
    state.terrain.begin(world::java::Dimension {
        session: 7,
        epoch: 2,
        name: "fixture:other".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    assert!(
        controller.view_sample(&state).is_none(),
        "a newly received dimension cannot sample old history before its first movement frame"
    );
    controller.begin_frame(&state, Duration::ZERO);
    assert!(
        controller.view_sample(&state).is_none(),
        "old dimension cannot publish a camera"
    );
}
