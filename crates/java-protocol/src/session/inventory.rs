//! Native container authority packets. Partial item/container snapshots are never emitted.
mod components;
use crate::{ProtocolError as Error, nbt, target, wire::Reader};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stack {
    pub item: u32,
    pub count: u32,
    /// A component patch overrides defaults; absent entries do not mean absent components.
    pub added: BTreeMap<&'static str, Value>,
    pub removed: BTreeSet<&'static str>,
}
#[derive(Clone, Debug, Serialize)]
pub enum Packet {
    Content {
        container: i32,
        state: i32,
        slots: Vec<Option<Stack>>,
        carried: Option<Stack>,
    },
    Slot {
        container: i32,
        state: i32,
        slot: i16,
        stack: Option<Stack>,
    },
    PlayerSlot {
        slot: i32,
        stack: Option<Stack>,
    },
    Cursor(Option<Stack>),
    Selected(i32),
    Open {
        container: i32,
        kind: &'static str,
        title: nbt::Value,
    },
    Close(i32),
    Property {
        container: i32,
        property: i16,
        value: i16,
    },
}
pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, Error> {
    Ok(Some(match name {
        "ClientboundContainerSetContentPacket" => {
            let container = nonnegative(r)?;
            let state = nonnegative(r)?;
            // Bound the expanded slot array as well as its wire bytes. This is an explicit
            // provisional admission budget, not a claim about modded container capacity.
            let count = r.count(crate::stream::MAX_UNCOMPRESSED / size_of::<Option<Stack>>())?;
            let mut slots = Vec::with_capacity(count);
            for _ in 0..count {
                slots.push(stack(r)?);
            }
            Packet::Content {
                container,
                state,
                slots,
                carried: stack(r)?,
            }
        }
        "ClientboundContainerSetSlotPacket" => Packet::Slot {
            container: nonnegative(r)?,
            state: nonnegative(r)?,
            slot: short(r)?,
            stack: stack(r)?,
        },
        "ClientboundSetPlayerInventoryPacket" => Packet::PlayerSlot {
            slot: nonnegative(r)?,
            stack: stack(r)?,
        },
        "ClientboundSetCursorItemPacket" => Packet::Cursor(stack(r)?),
        "ClientboundSetHeldSlotPacket" => Packet::Selected(nonnegative(r)?),
        "ClientboundOpenScreenPacket" => {
            let container = nonnegative(r)?;
            let id = nonnegative(r)? as usize;
            let kind = target::CONTAINER_TYPES
                .get(id)
                .copied()
                .ok_or(Error::Invalid("unknown native menu type"))?;
            Packet::Open {
                container,
                kind,
                title: nbt::read(r)?,
            }
        }
        "ClientboundContainerClosePacket" => Packet::Close(nonnegative(r)?),
        "ClientboundContainerSetDataPacket" => Packet::Property {
            container: nonnegative(r)?,
            property: short(r)?,
            value: short(r)?,
        },
        _ => return Ok(None),
    }))
}
fn short(r: &mut Reader<'_>) -> Result<i16, Error> {
    Ok(i16::from_be_bytes(r.take(2)?.try_into().unwrap()))
}
fn nonnegative(r: &mut Reader<'_>) -> Result<i32, Error> {
    let n = r.var()?;
    if n < 0 {
        Err(Error::Invalid("negative native inventory identifier"))
    } else {
        Ok(n)
    }
}
fn component(r: &mut Reader<'_>) -> Result<&'static target::ItemComponent, Error> {
    target::ITEM_COMPONENTS
        .get(nonnegative(r)? as usize)
        .ok_or(Error::Invalid("unknown native item component"))
}
pub(super) fn stack(r: &mut Reader<'_>) -> Result<Option<Stack>, Error> {
    let count = r.var()?;
    // The pinned optional-item codec treats nonpositive counts as empty, without further fields.
    if count <= 0 {
        return Ok(None);
    }
    let item = nonnegative(r)? as u32;
    if target::ITEM_TYPES.get(item as usize).is_none() {
        return Err(Error::Invalid("unknown native item"));
    }
    let added_count = r.count(target::ITEM_COMPONENTS.len())?;
    let removed_count = r.count(target::ITEM_COMPONENTS.len())?;
    if added_count + removed_count > target::ITEM_COMPONENTS.len() {
        return Err(Error::Invalid("duplicate native item components"));
    }
    let mut added = BTreeMap::new();
    let mut removed = BTreeSet::new();
    for _ in 0..added_count {
        let spec = component(r)?;
        let value = components::read(spec, r)?;
        if added.insert(spec.name, value).is_some() {
            return Err(Error::Invalid("duplicate added item component"));
        }
    }
    for _ in 0..removed_count {
        let name = component(r)?.name;
        if added.contains_key(name) || !removed.insert(name) {
            return Err(Error::Invalid("duplicate removed item component"));
        }
    }
    Ok(Some(Stack {
        item,
        count: count as u32,
        added,
        removed,
    }))
}

#[cfg(test)]
mod tests;
