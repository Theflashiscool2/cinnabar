//! Entity item appearances (equipment and ground stacks). Reuses inventory's stack model; this is not a
//! second transactional inventory. Keys are admitted entity incarnations.
mod pickup;
mod remote_use;
use inventory::java::Slot;
pub(super) use pickup::take;
pub use pickup::{Collector, Pickup};
pub(in crate::java) use remote_use::observe as observe_remote_use;
pub use remote_use::pose as player_use_pose;
use std::collections::BTreeMap;
use world::java::entities::Identity;
#[derive(Clone, Debug, Default)]
pub struct Equipment {
    /// Native MAINHAND, OFFHAND, FEET, LEGS, CHEST, HEAD, BODY, SADDLE order.
    pub slots: [Slot; 8],
    pub revision: u64,
}
#[derive(Clone, Debug)]
pub struct Ground {
    pub stack: Slot,
    pub revision: u64,
}
#[derive(Debug, Default)]
pub struct State {
    ground: BTreeMap<Identity, Ground>,
    entries: BTreeMap<Identity, Equipment>,
    revision: u64,
    pickups: Vec<Pickup>,
}
impl State {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn get(&self, identity: Identity) -> Option<&Equipment> {
        self.entries.get(&identity)
    }
    pub fn ground(&self, identity: Identity) -> Option<&Ground> {
        self.ground.get(&identity)
    }
    pub(super) fn set_ground(
        &mut self,
        identity: Identity,
        stack: Slot,
    ) -> Result<(), &'static str> {
        if self
            .ground
            .get(&identity)
            .is_some_and(|old| old.stack == stack)
        {
            return Ok(());
        }
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("Java ground item revision exhausted")?;
        self.ground.insert(
            identity,
            Ground {
                stack,
                revision: self.revision,
            },
        );
        Ok(())
    }
    pub(super) fn appearance_changed(&mut self, identity: Identity) {
        if self.entries.contains_key(&identity) {
            self.revision = self.revision.saturating_add(1);
        }
    }
    pub(super) fn clear(&mut self) {
        self.pickups.clear();
        self.entries.clear();
        self.ground.clear();
        self.revision = self.revision.saturating_add(1);
    }
    pub(super) fn remove(&mut self, identity: Identity) {
        let equipment = self.entries.remove(&identity).is_some();
        let ground = self.ground.remove(&identity).is_some();
        if equipment || ground {
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
    let observed = state.equipment.get(identity).map(|gear| {
        [0, 1].map(|slot| {
            gear.slots[slot]
                .as_ref()
                .filter(|stack| stack.count > 0)
                .map(|stack| std::sync::Arc::<str>::from(stack.item.as_str()))
        })
    });
    state.entities.update(scope, packet.id, |entity| {
        if let Some(using) = &mut entity.visual.remote_use {
            using.observed_item = observed
                .as_ref()
                .and_then(|items| items[usize::from(entity.visual.use_flags & 2 != 0)].clone());
        }
        Ok(())
    })
}
