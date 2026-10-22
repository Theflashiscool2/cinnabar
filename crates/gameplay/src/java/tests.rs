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
    controller.begin_frame(&world(2), TICK);
    assert!(controller.pending().is_none());
    assert!(controller.last_position.is_none());
    assert!(!controller.body.on_ground);
}

#[test]
fn session_replacement_invalidates_unsent_prediction_without_replacing_motion() {
    let mut controller = Controller::default();
    let mut state = world(1);
    controller.begin_frame(&state, TICK);
    controller.pending = Some(frame());
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
    // Camera publication has not run yet: its previous 90/25 angles are stale even
    // though the network acknowledgement has completed. The caller must replace them.
    assert!(controller.begin_frame(&state, TICK));
    assert_eq!(state.player_motion.unwrap().pitch, 45.);
    assert!(controller.pending().is_none());
    assert!(controller.last_position.is_none());
    assert!(controller.last_input.is_none());
    assert!(controller.last_sprint.is_none());
    assert!(!controller.begin_frame(&state, TICK));
}
