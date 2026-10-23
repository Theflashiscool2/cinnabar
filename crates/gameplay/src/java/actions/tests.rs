use super::*;
use semantic_input::{ActionSnapshot, InputMode, identity::InputSequence};

fn state() -> WorldState {
    let mut state = WorldState::default();
    state.begin(7);
    state.terrain.begin(world::java::Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:selection".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    state.inventory.begin(7, 1);
    state.vitals = Some(java_protocol::session::facts::Vitals {
        health: 20.,
        food: 20,
        saturation: 5.,
    });
    state
}
fn input(sequence: u64, action: Option<Action>) -> FinalizedInput {
    let mut phases = [ActionPhase::default(); Action::COUNT];
    if let Some(action) = action {
        phases[action as usize] = ActionPhase {
            pressed: true,
            held: true,
            released: false,
        };
    }
    FinalizedInput::from_finalized(ActionSnapshot {
        ordered: None,
        frame_sequence: InputSequence::new(sequence),
        authority_generation: InputAuthorityGeneration::INITIAL,
        movement: [0.; 2],
        raw_movement: [0.; 2],
        analogue_movement: [0.; 2],
        movement_buttons: Default::default(),
        look_delta: [0.; 2],
        input_mode: InputMode::KeyboardMouse,
        phases,
        release_reasons: [None; Action::COUNT],
        authority_released: false,
    })
}
#[test]
fn choices_wait_for_serialization_and_preserve_attack_order_under_backpressure() {
    let mut state = state();
    let mut c = Controller::default();
    c.capture(&state, &input(1, None), None);
    c.append(
        [
            Intent::Attack(true),
            Intent::Slot(8),
            Intent::Attack(false),
            Intent::Cycle(1),
        ],
        None,
    );
    assert_eq!(c.front(&state), Some(Intent::Attack(true)));
    assert!(c.consume_attack(true).0.pressed);
    let request = c.selection(&state).unwrap();
    assert_eq!((request.slot, request.sequence), (8, 1));
    // A full transport has not consumed the choice or changed the inventory.
    assert_eq!(c.selection(&state), Some(request));
    assert_eq!(state.inventory.selected, Some(0));
    c.selection_queued();
    assert_eq!(c.front(&state), None);
    assert!(!c.held_attack().held);
    assert!(state.inventory.selection_sent(7, 1, 1, 8));
    assert_eq!(c.front(&state), Some(Intent::Attack(false)));
    assert!(c.consume_attack(false).0.released);
    assert_eq!(c.selection(&state).unwrap().slot, 0);
    c.selection_queued();
    assert!(state.inventory.selection_sent(7, 1, 2, 0));
    assert_eq!(c.front(&state), None);
    assert_eq!(c.backlog(), 0);
}
#[test]
fn server_selection_between_writes_changes_the_cycle_baseline() {
    let mut state = state();
    let mut c = Controller::default();
    c.capture(&state, &input(1, Some(Action::Hotbar9)), None);
    c.selection(&state).unwrap();
    c.selection_queued();
    state.inventory.selection_sent(7, 1, 1, 8);
    state
        .inventory
        .admit(7, 1, inventory::java::Update::Selected(4))
        .unwrap();
    c.capture(&state, &input(2, Some(Action::HotbarPrevious)), None);
    assert_eq!(c.selection(&state).unwrap().slot, 3);
}
#[test]
fn repeated_frame_and_authority_loss_cannot_replay_a_choice() {
    let state = state();
    let mut c = Controller::default();
    let frame = input(1, Some(Action::Hotbar2));
    c.capture(&state, &frame, None);
    c.capture(&state, &frame, None);
    assert_eq!(c.backlog(), 1);
    assert_eq!(c.selection(&state).unwrap().slot, 1);
    c.capture(&state, &FinalizedInput::default(), None);
    assert_eq!(c.backlog(), 0);
    assert!(c.selection(&state).is_none());
    c.capture(&state, &input(2, Some(Action::Hotbar3)), None);
    assert_eq!(c.selection(&state).unwrap().sequence, 1);
}
#[test]
fn overflow_is_explicit_and_quarantines_until_neutral() {
    let state = state();
    let mut c = Controller::default();
    c.capture(&state, &input(1, None), None);
    c.append(
        std::iter::repeat_n(Intent::Cycle(1), MAX_PENDING_ACTION_EVENTS + 1),
        None,
    );
    assert_eq!(c.backlog(), 0);
    assert!(c.blocked.is_some());
    c.capture(&state, &input(2, Some(Action::Hotbar2)), None);
    assert_eq!(c.backlog(), 0);
    c.capture(&state, &input(3, None), None);
    assert!(c.blocked.is_none());
    c.capture(&state, &input(4, Some(Action::Hotbar2)), None);
    assert_eq!(c.selection(&state).unwrap().slot, 1);
}
#[test]
fn dimension_replacement_clears_pending_edges_and_receipts() {
    let mut state = state();
    let mut c = Controller::default();
    c.capture(&state, &input(1, Some(Action::Hotbar2)), None);
    c.selection(&state).unwrap();
    c.selection_queued();
    let mut dimension = state.terrain.dimension().unwrap().clone();
    dimension.epoch = 2;
    state.terrain.begin(dimension);
    state.inventory.begin(7, 2);
    // A dimension replacement may be admitted before the next physical input frame.
    // The already consumed frame belongs to the old world, even if its button was pressed.
    c.capture(&state, &input(1, Some(Action::Hotbar2)), None);
    assert_eq!(c.backlog(), 0);
    c.capture(&state, &input(2, None), None);
    assert_eq!(c.backlog(), 0);
    assert!(!state.inventory.selection_sent(7, 1, 1, 1));
    assert_eq!(state.inventory.selected, Some(0));
}

#[test]
fn actual_desktop_router_keeps_attack_then_digit_then_scroll_order() {
    use semantic_input::{
        DesktopEvent, DesktopEvents, DeviceFrame, KeyboardMouseFrame, PhysicalControl,
        SemanticInputRouter,
    };
    let mut events = DesktopEvents::default();
    for (control, pressed) in [
        (PhysicalControl::MouseButton(1), true),
        (PhysicalControl::KeyboardUsage(0x26), true),
        (PhysicalControl::KeyboardUsage(0x26), false),
        (PhysicalControl::MouseButton(1), false),
    ] {
        events.push(DesktopEvent::Button {
            control,
            pressed,
            modifiers: Default::default(),
        });
    }
    events.push(DesktopEvent::Scroll { next: false });
    let mut router = SemanticInputRouter::default();
    router
        .route(DeviceFrame {
            desktop_events: Some(events),
            keyboard_mouse: Some(KeyboardMouseFrame {
                activity_sequence: 1,
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
    let frame = FinalizedInput::from_finalized(router.finalize().unwrap());
    let mut c = Controller::default();
    c.capture(&state(), &frame, None);
    assert_eq!(
        c.queue.iter().map(|e| e.intent).collect::<Vec<_>>(),
        [
            Intent::Attack(true),
            Intent::Slot(8),
            Intent::Attack(false),
            Intent::Cycle(-1)
        ]
    );
}

#[test]
fn a_click_retains_its_target_while_selection_waits_for_the_socket() {
    let mut state = state();
    let mut c = Controller::default();
    c.capture(&state, &input(1, Some(Action::Hotbar2)), None);
    let original = Hit {
        block: [1, -60, 2],
        state: 10,
        position: [1.5, -59., 2.5],
        face: sim::java::selection::Face::Up,
        inside: false,
    };
    c.capture(&state, &input(2, Some(Action::Attack)), Some(original));
    c.selection(&state).unwrap();
    c.selection_queued();
    // Later camera frames cannot overwrite the buffered click's world position.
    c.capture(
        &state,
        &input(3, None),
        Some(Hit {
            block: [9, -60, 2],
            ..original
        }),
    );
    assert!(c.front(&state).is_none());
    state.inventory.selection_sent(7, 1, 1, 1);
    assert_eq!(c.front(&state), Some(Intent::Attack(true)));
    let (phase, target) = c.consume_attack(true);
    assert!(phase.pressed);
    assert_eq!(target.unwrap().block, original.block);
}
