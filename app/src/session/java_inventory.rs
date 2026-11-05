//! Native inventory intent binding. Shared UI never writes a Java packet or a Bedrock ledger.
use super::SessionController;
use client_ui::ui_runtime::{
    UiRuntime,
    java_inventory::{Input, Intent},
};
use inventory::java::{Slot, pickup::Button};
use java_protocol::{
    session::{Command, inventory::outbound},
    target,
};

pub(super) fn flush(controller: &mut SessionController, ui: &mut UiRuntime) {
    let Some(java) = controller.java.as_mut() else {
        return;
    };
    for _ in 0..32 {
        let Some(input) = ui.next_java_inventory_input() else {
            break;
        };
        let result = prepare(input, &controller.java_world.inventory);
        let command = match result {
            Ok((command, missing)) => {
                if missing > 0 {
                    java.diagnostic_notice("java_inventory_prediction_requires_server_resync");
                }
                command
            }
            Err(error) => {
                bevy::log::warn!(target:"java_session", %error, "Java inventory action refused");
                ui.finish_java_inventory_input(Some(error));
                continue;
            }
        };
        match java.command(command) {
            Ok(()) => {
                bevy::log::debug!(target: "java_session", sequence=input.sequence,
                    container=input.identity.container, intent=?input.intent,
                    "Native Java inventory input queued");
                ui.finish_java_inventory_input(None);
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => break,
            Err(_) => {
                ui.finish_java_inventory_input(Some("Java session input closed".into()));
                break;
            }
        }
    }
}
fn prepare(input: &Input, state: &inventory::java::State) -> Result<(Command, usize), String> {
    let id = input.identity;
    if state.active_identity() != Some(id) {
        return Err("Java inventory scope retired".into());
    }
    if matches!(input.intent, Intent::Close) {
        return Ok((
            Command::ContainerClose {
                dimension_epoch: id.epoch,
                incarnation: id.incarnation,
                sequence: input.sequence,
                container: id.container,
            },
            0,
        ));
    }
    if let Intent::DropCursor { button } = input.intent {
        let (revision, carried) = state.observe_carried(id)?;
        if carried.is_none() {
            return Err("Java carried stack is empty".into());
        }
        let (carried, missing) = hash(carried)?;
        let click = outbound::observed_pickup(
            id.container,
            revision,
            outbound::SLOT_CLICKED_OUTSIDE,
            u8::from(button == Button::Right),
            carried,
        )
        .map_err(|e| e.to_string())?;
        return Ok((
            Command::ContainerClick {
                dimension_epoch: id.epoch,
                incarnation: id.incarnation,
                sequence: input.sequence,
                click,
            },
            missing,
        ));
    }
    if let Intent::QuickCraft {
        ref slots,
        one_each,
    } = input.intent
    {
        let first = *slots.first().ok_or("Java drag has no slots")?;
        let (revision, carried) = state.observe_storage_slot(id, first)?;
        for slot in slots {
            state.observe_storage_slot(id, *slot)?;
        }
        if let Some(stack) = carried {
            max_stack(stack)?;
        }
        let (carried, missing) = hash(carried)?;
        let carried = carried.ok_or("Java drag carried stack is not ready")?;
        let slots = slots
            .iter()
            .map(|slot| i16::try_from(*slot).map_err(|_| "Java drag slot exceeds wire range"))
            .collect::<Result<Vec<_>, _>>()?;
        let click = outbound::quick_craft(id.container, revision, &slots, one_each, carried)
            .map_err(|e| e.to_string())?;
        return Ok((
            Command::ContainerClick {
                dimension_epoch: id.epoch,
                incarnation: id.incarnation,
                sequence: input.sequence,
                click,
            },
            missing,
        ));
    }
    let slot = match input.intent {
        Intent::Pickup { slot, .. }
        | Intent::QuickMove { slot }
        | Intent::Collect { slot }
        | Intent::Swap { slot, .. }
        | Intent::Throw { slot, .. } => slot,
        Intent::Close | Intent::QuickCraft { .. } | Intent::DropCursor { .. } => unreachable!(),
    };
    let (revision, carried) = state.observe_storage_slot(id, slot)?;
    if let Intent::Pickup { button, .. } = input.intent {
        state.plan_pickup(id, slot, button, max_stack)?;
    }
    // Throw applies only to a hovered slot with an empty cursor. Never misreport a cursor drop.
    if matches!(input.intent, Intent::Throw { .. }) && carried.is_some() {
        return Err("Place the carried stack before dropping a hovered item".into());
    }
    let (carried, missing) = hash(carried)?;
    let slot = i16::try_from(slot).map_err(|_| "Java slot exceeds wire range")?;
    // We install no local prediction. Report the unchanged client view so vanilla sends deltas.
    let click = match input.intent {
        Intent::Pickup { button, .. } => outbound::observed_pickup(
            id.container,
            revision,
            slot,
            u8::from(button == Button::Right),
            carried,
        ),
        Intent::QuickMove { .. } => outbound::observed_action(
            id.container,
            revision,
            slot,
            outbound::ObservedAction::QuickMove,
            carried,
        ),
        Intent::Collect { .. } => outbound::observed_action(
            id.container,
            revision,
            slot,
            outbound::ObservedAction::Collect,
            carried,
        ),
        Intent::Swap { hotbar, .. } => outbound::observed_action(
            id.container,
            revision,
            slot,
            outbound::ObservedAction::HotbarSwap(hotbar),
            carried,
        ),
        Intent::Throw { whole_stack, .. } => outbound::observed_action(
            id.container,
            revision,
            slot,
            outbound::ObservedAction::Throw { whole_stack },
            carried,
        ),
        Intent::Close | Intent::QuickCraft { .. } | Intent::DropCursor { .. } => unreachable!(),
    }
    .map_err(|e| e.to_string())?;
    Ok((
        Command::ContainerClick {
            dimension_epoch: id.epoch,
            incarnation: id.incarnation,
            sequence: input.sequence,
            click,
        },
        missing,
    ))
}
fn hash(slot: &Slot) -> Result<(Option<outbound::HashedStack>, usize), String> {
    let Some(stack) = slot else {
        return Ok((None, 0));
    };
    let item = target::ITEM_TYPES
        .iter()
        .position(|name| *name == stack.item)
        .ok_or("unknown Java item")? as u32;
    let (hash, missing) =
        outbound::HashedStack::predicted(item, stack.count, &stack.added, &stack.removed)
            .map_err(|e| e.to_string())?;
    Ok((Some(hash), missing))
}

/// Resolves native default and patched stack limits for both pointer eligibility and packets.
pub(crate) fn max_stack(stack: &inventory::java::Stack) -> Result<u32, String> {
    if let Some(limit) = stack.added.get("minecraft:max_stack_size") {
        return limit
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0 && *v <= 99)
            .ok_or("invalid Java stack limit".into());
    }
    if stack.removed.contains("minecraft:max_stack_size") {
        return Err("missing Java stack limit".into());
    }
    let id = target::ITEM_TYPES
        .iter()
        .position(|name| *name == stack.item)
        .ok_or("unknown Java item")?;
    Ok(target::ITEM_STACK_LIMITS[id])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_drop_preserves_authority_and_rejects_replaced_menus() {
        let mut state = inventory::java::State::default();
        state.begin(7, 2);
        state
            .admit(
                7,
                2,
                inventory::java::Update::Content {
                    container: 0,
                    state: 19,
                    slots: vec![None; inventory::java::PLAYER_MENU_SIZE],
                    carried: Some(inventory::java::Stack {
                        item: "minecraft:emerald".into(),
                        count: 8,
                        added: Default::default(),
                        removed: Default::default(),
                    }),
                },
            )
            .unwrap();
        let id = state.active_identity().unwrap();
        let before = serde_json::to_value(&state).unwrap();
        for button in [Button::Left, Button::Right] {
            let input = Input {
                identity: id,
                sequence: 1,
                intent: Intent::DropCursor { button },
            };
            let (Command::ContainerClick { click, .. }, 0) = prepare(&input, &state).unwrap()
            else {
                panic!("not an ordinary native click")
            };
            assert_eq!(
                &click.payload()[..7],
                &[0, 19, 0xfc, 0x19, u8::from(button == Button::Right), 0, 0]
            );
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        }
        state.begin(7, 3);
        assert!(
            prepare(
                &Input {
                    identity: id,
                    sequence: 2,
                    intent: Intent::DropCursor {
                        button: Button::Left
                    }
                },
                &state
            )
            .is_err()
        );
    }
}
