//! Optional tag registries must not gate the entire destroy-block action.
use super::*;

fn prepare(item: Option<&str>, explicit_empty: bool) -> (WorldState, Controller, Hit) {
    let mut state = mining_state();
    state.bootstrap.as_mut().unwrap().configuration.tags.clear();
    if explicit_empty {
        state
            .bootstrap
            .as_mut()
            .unwrap()
            .configuration
            .tags
            .insert("minecraft:block".into(), Default::default());
    }
    state
        .inventory
        .admit(
            7,
            1,
            inventory::java::Update::PlayerSlot {
                slot: 0,
                stack: item.map(|name| inventory::java::Stack {
                    item: name.into(),
                    count: 1,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
    let mut controller = Controller::default();
    begin(&mut controller, &state, Duration::ZERO);
    controller.contact = Some((true, false));
    let mut target = hit();
    target.state = 1;
    (state, controller, target)
}

#[test]
fn omitted_block_tags_and_explicit_empty_tags_have_identical_mining_lifetimes() {
    for item in [
        None,
        Some("minecraft:wooden_sword"),
        Some("minecraft:iron_pickaxe"),
    ] {
        let mut finishes = Vec::new();
        for explicit_empty in [false, true] {
            let (state, mut controller, target) = prepare(item, explicit_empty);
            controller.advance(&state, press(), Some(target));
            assert_eq!(controller.pending().map(|p| p.kind), Some(Kind::Break));
            controller.queued();
            let mut finish_tick = None;
            for tick in 1..=200 {
                begin(&mut controller, &state, simulation_clock::TICK_DURATION);
                controller.advance(
                    &state,
                    ActionPhase {
                        held: true,
                        ..Default::default()
                    },
                    Some(target),
                );
                if let Some(request) = controller.pending() {
                    assert_eq!(request.kind, Kind::FinishBreak);
                    finish_tick = Some(tick);
                    break;
                }
            }
            assert!(
                finish_tick.is_some(),
                "known hardness/default tool speed must finish"
            );
            finishes.push(finish_tick);
            assert_eq!(state.block_updates, 0, "intent never edits terrain");
        }
        assert_eq!(finishes[0], finishes[1]);
    }
}

#[test]
fn late_authoritative_tags_change_tool_speed_and_empty_replacement_clears_it() {
    let (mut state, mut controller, target) = prepare(Some("minecraft:iron_pickaxe"), false);
    controller.advance(&state, press(), Some(target));
    assert_eq!(controller.pending().map(|p| p.kind), Some(Kind::Break));
    controller.queued();
    let slow = controller.increment(&state, target).unwrap();
    for (tags, faster) in [
        (
            [("minecraft:mineable/pickaxe".into(), vec![1])].into(),
            true,
        ),
        (Default::default(), false),
    ] {
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Tags([("minecraft:block".into(), tags)].into()),
        }));
        let increment = controller.increment(&state, target).unwrap();
        if faster {
            assert!(increment > slow);
        } else {
            assert_eq!(increment, slow);
        }
    }
    controller.advance(
        &state,
        ActionPhase {
            released: true,
            ..Default::default()
        },
        Some(target),
    );
    assert_eq!(controller.pending().map(|p| p.kind), Some(Kind::AbortBreak));
    assert_eq!(state.block_updates, 0);
}
