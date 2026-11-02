use super::poll_event;
use client_session::java::Session;
use client_world::java::WorldState;
use java_protocol::session::{Envelope, Event, facts::Teleport};
fn correction() -> WorldState {
    let mut state = WorldState::default();
    state.begin(7);
    state.terrain.begin(world::java::Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:world".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: Teleport {
                id: 9,
                position: [1., 64., 2.],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: 0
            }
        }
    }));
    assert!(state.correction_command().is_some());
    state
}
#[test]
fn closed_correction_input_still_drains_events_and_preserves_terminal_reason() {
    let mut state = correction();
    let (ready, rx) = std::sync::mpsc::channel();
    let mut session = Session::run(move |events, input| async move {
        drop(input);
        events
            .send(Envelope {
                generation: 7,
                event: Event::KeepAlive(4),
            })
            .await
            .unwrap();
        ready.send(()).unwrap();
        Err("fixture server disconnect reason".into())
    });
    test_time::eventually("session input closed after publishing event", || {
        rx.try_recv().is_ok()
    });
    assert!(matches!(
        poll_event(&mut session, &mut state).unwrap().event,
        Event::KeepAlive(4)
    ));
    assert!(state.terrain_error.is_none());
    let mut outcome = None;
    test_time::eventually("original terminal reason after FIFO drains", || {
        outcome = session.result();
        outcome.is_some()
    });
    assert_eq!(
        outcome.unwrap().unwrap_err(),
        "fixture server disconnect reason"
    );
}
#[test]
fn full_correction_input_still_drains_events_and_retries_unsent_authority() {
    let mut state = correction();
    let (ready, rx) = std::sync::mpsc::channel();
    let (finish, wait) = tokio::sync::oneshot::channel::<()>();
    let mut session = Session::run(move |events, input| async move {
        events
            .send(Envelope {
                generation: 7,
                event: Event::KeepAlive(5),
            })
            .await
            .unwrap();
        ready.send(()).unwrap();
        let _ = wait.await;
        drop(input);
        Ok(())
    });
    test_time::eventually("session holds input open", || rx.try_recv().is_ok());
    for _ in 0..32 {
        session
            .command(state.correction_command().unwrap().1)
            .unwrap();
    }
    assert!(matches!(
        poll_event(&mut session, &mut state).unwrap().event,
        Event::KeepAlive(5)
    ));
    assert!(state.correction_command().is_some());
    assert!(state.terrain_error.is_none());
    let _ = finish.send(());
}
