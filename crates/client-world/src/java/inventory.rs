//! Converts native wire snapshots to inventory-owned state before any input can observe them.
use super::WorldState;
use ::inventory::java::{Stack, Update};
use java_protocol::{session::inventory as wire, target};

fn stack(value: Option<wire::Stack>) -> Result<Option<Stack>, &'static str> {
    value
        .map(|s| {
            Ok(Stack {
                item: target::ITEM_TYPES
                    .get(s.item as usize)
                    .ok_or("unknown native inventory item")?
                    .to_string(),
                count: s.count,
                added: s
                    .added
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect(),
                removed: s.removed.into_iter().map(str::to_owned).collect(),
            })
        })
        .transpose()
}
pub(super) fn admit(state: &mut WorldState, epoch: u64, packet: wire::Packet) -> bool {
    if !state.inventory.accepts(state.generation(), epoch) {
        return false;
    }
    let update = match convert(packet) {
        Ok(update) => update,
        Err(error) => {
            state.inventory.error = Some(error.into());
            return false;
        }
    };
    state
        .inventory
        .admit(state.generation(), epoch, update)
        .unwrap_or(false)
}
fn convert(packet: wire::Packet) -> Result<Update, &'static str> {
    Ok(match packet {
        wire::Packet::Content {
            container,
            state,
            slots,
            carried,
        } => Update::Content {
            container,
            state,
            slots: slots.into_iter().map(stack).collect::<Result<_, _>>()?,
            carried: stack(carried)?,
        },
        wire::Packet::Slot {
            container,
            state,
            slot,
            stack: item,
        } => {
            let slot = usize::try_from(slot).map_err(|_| "negative native inventory slot")?;
            Update::Slot {
                container,
                state,
                slot,
                stack: stack(item)?,
            }
        }
        wire::Packet::PlayerSlot { slot, stack: item } => {
            let slot = usize::try_from(slot).map_err(|_| "negative native inventory slot")?;
            Update::PlayerSlot {
                slot,
                stack: stack(item)?,
            }
        }
        wire::Packet::Cursor(item) => Update::Cursor(stack(item)?),
        wire::Packet::Selected(slot) => {
            let slot = usize::try_from(slot).map_err(|_| "negative native inventory slot")?;
            Update::Selected(slot)
        }
        wire::Packet::Open {
            container,
            kind,
            title,
        } => Update::Open {
            container,
            kind: kind.into(),
            title: serde_json::to_value(title).map_err(|_| "invalid native menu title")?,
        },
        wire::Packet::Close(container) => Update::Close(container),
        wire::Packet::Property {
            container,
            property,
            value,
        } => Update::Property {
            container,
            property,
            value,
        },
    })
}

#[cfg(test)]
mod tests;
