//! Entity equipment authority. Reuses inventory's stack model; this is not a
//! second transactional inventory. Keys are admitted entity incarnations.
use inventory::java::Slot;
use std::collections::BTreeMap;
use world::java::entities::Identity;
#[derive(Clone, Debug, Default)]
pub struct Equipment {
    /// Native MAINHAND, OFFHAND, FEET, LEGS, CHEST, HEAD, BODY, SADDLE order.
    pub slots: [Slot; 8],
    pub revision: u64,
}
#[derive(Debug, Default)]
pub struct State {
    entries: BTreeMap<Identity, Equipment>,
    revision: u64,
}
impl State {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn get(&self, identity: Identity) -> Option<&Equipment> {
        self.entries.get(&identity)
    }
    pub(super) fn appearance_changed(&mut self, identity: Identity) {
        if self.entries.contains_key(&identity) {
            self.revision = self.revision.saturating_add(1);
        }
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.revision = self.revision.saturating_add(1);
    }
    pub(super) fn remove(&mut self, identity: Identity) {
        if self.entries.remove(&identity).is_some() {
            self.revision = self.revision.saturating_add(1);
        }
    }
    fn apply(&mut self, identity: Identity, slots: BTreeMap<u8, Slot>) -> Result<(), &'static str> {
        if slots.keys().any(|slot| usize::from(*slot) >= 8) {
            return Err("Invalid Java equipment slot");
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Java equipment revision exhausted")?;
        let entry = self.entries.entry(identity).or_default();
        for (slot, stack) in slots {
            entry.slots[usize::from(slot)] = stack;
        }
        entry.revision = revision;
        self.revision = revision;
        Ok(())
    }
}
pub(super) fn admit(
    state: &mut super::WorldState,
    scope: (u64, u64),
    packet: java_protocol::session::entities::equipment::Packet,
) -> Result<bool, &'static str> {
    let Some(entity) = state.entities.get(packet.id) else {
        return Ok(false);
    };
    if packet.lifetime == 0 || entity.transport_lifetime != packet.lifetime {
        return Ok(false);
    }
    let identity = entity.identity;
    let slots = packet
        .slots
        .into_iter()
        .map(|(slot, stack)| super::inventory::stack(stack).map(|stack| (slot, stack)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    state.equipment.apply(identity, slots)?;
    state.entities.update(scope, packet.id, |_| Ok(()))
}
