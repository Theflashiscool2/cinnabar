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
    Ok(match input.intent {
        Intent::Close => (
            Command::ContainerClose {
                dimension_epoch: id.epoch,
                incarnation: id.incarnation,
                sequence: input.sequence,
                container: id.container,
            },
            0,
        ),
        Intent::Pickup { slot, button } => {
            let plan = state.plan_pickup(id, slot, button, |stack| {
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
            let mut missing = 0;
            let mut hash = |slot: &Slot| -> Result<_, String> {
                slot.as_ref()
                    .map(|stack| {
                        let item = target::ITEM_TYPES
                            .iter()
                            .position(|name| *name == stack.item)
                            .ok_or("unknown Java item")? as u32;
                        let (hash, absent) = outbound::HashedStack::predicted(
                            item,
                            stack.count,
                            &stack.added,
                            &stack.removed,
                        )
                        .map_err(|e| e.to_string())?;
                        missing += absent;
                        Ok(hash)
                    })
                    .transpose()
            };
            // No local prediction is installed. Report the actual unchanged client view:
            // vanilla's remote-slot cache will then send the authoritative slot/cursor deltas.
            // Claiming plan.changed/plan.carried suppresses those updates and leaves stale UI.
            let carried = state
                .menu(id.container)
                .and_then(|m| m.carried.as_ref())
                .ok_or("Java carried stack is not ready")?;
            let click = outbound::observed_pickup(
                id.container,
                plan.state,
                slot as i16,
                u8::from(button == Button::Right),
                hash(carried)?,
            )
            .map_err(|e| e.to_string())?;
            (
                Command::ContainerClick {
                    dimension_epoch: id.epoch,
                    incarnation: id.incarnation,
                    sequence: input.sequence,
                    click,
                },
                missing,
            )
        }
    })
}
