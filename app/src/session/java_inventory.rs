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
            Ok(()) => ui.finish_java_inventory_input(None),
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
    let slot = match input.intent {
        Intent::Pickup { slot, .. }
        | Intent::QuickMove { slot }
        | Intent::Swap { slot, .. }
        | Intent::Throw { slot, .. } => slot,
        Intent::Close => unreachable!(),
    };
    let (revision, carried) = state.observe_storage_slot(id, slot)?;
    if let Intent::Pickup { button, .. } = input.intent {
        state.plan_pickup(id, slot, button, |stack| {
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
        })?;
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
        Intent::Close => unreachable!(),
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
