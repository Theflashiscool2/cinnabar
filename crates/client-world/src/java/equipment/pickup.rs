//! Authoritative collection snapshots; inventory awards remain the container protocol's owner.
use super::*;
use world::java::entities::{Entity, item_motion};

#[derive(Clone, Copy, Debug)]
pub enum Collector {
    Local(i32),
    Remote { network_id: i32, identity: Identity },
}
#[derive(Clone, Debug)]
pub struct Pickup {
    pub identity: Identity,
    pub source: Identity,
    pub source_network_id: i32,
    pub uuid: [u8; 16],
    pub revision: u64,
    pub collector: Collector,
    pub ground: Ground,
    pub flight: item_motion::Pickup,
    pub age: f32,
    pending_local_target: bool,
    source_depleted: bool,
}
impl State {
    pub fn pickups(&self) -> impl Iterator<Item = &Pickup> {
        self.pickups.iter()
    }
    /// An ordinary despawn/reused ID cannot leave an animation targeting its replacement.
    pub(in crate::java) fn retire_pickup_network(&mut self, ids: &[i32], reused: bool) {
        let before = self.pickups.len();
        self.pickups.retain(|pickup| {
            let collector = match pickup.collector {
                Collector::Local(id) | Collector::Remote { network_id: id, .. } => id,
            };
            !(ids.contains(&pickup.source_network_id) && (reused || !pickup.source_depleted))
                && !ids.contains(&collector)
        });
        if self.pickups.len() != before {
            self.revision = self.revision.saturating_add(1);
        }
    }
    /// Uses the sole native client tick; target coordinates are presentation facts only.
    pub fn tick_pickups(
        &mut self,
        entities: &world::java::entities::Entities,
        local: Option<[f64; 3]>,
    ) {
        let before = self.pickups.len();
        self.pickups.retain_mut(|pickup| {
            let target = match pickup.collector {
                Collector::Local(_) => local,
                Collector::Remote {
                    network_id,
                    identity,
                } => entities
                    .get(network_id)
                    .filter(|entity| entity.identity == identity)
                    .and_then(target),
            };
            let Some(target) = target else {
                return false;
            };
            if pickup.pending_local_target {
                pickup.flight.initialize_target(target);
                pickup.pending_local_target = false;
            }
            pickup.flight.tick(target)
        });
        if self.pickups.len() != before {
            self.revision = self.revision.saturating_add(1);
        }
    }
    /// Resolves a local target before the first rendered frame, even when no tick is due.
    pub fn initialize_pickup_targets(&mut self, local: Option<[f64; 3]>) {
        if let Some(target) = local {
            for pickup in &mut self.pickups {
                if pickup.pending_local_target {
                    pickup.flight.initialize_target(target);
                    pickup.pending_local_target = false;
                }
            }
        }
    }
}
fn target(entity: &Entity) -> Option<[f64; 3]> {
    let mut position = entity
        .presentation
        .as_ref()
        .map_or(entity.motion.position, |p| p.track.current.position);
    let eye = if entity.kind.as_ref() == java_protocol::target::ENTITY_PLAYER {
        let mut body = sim::java::travel::Body::default();
        match entity.visual.pose.as_ref() {
            "standing" => {}
            "crouching" => body.crouched = true,
            "swimming" | "fall_flying" | "spin_attack" => body.low_profile = true,
            _ => return None,
        }
        body.eye_height(entity.visual.scale)
    } else {
        if entity.visual.pose.as_ref() != "standing" || entity.visual.baby {
            return None;
        }
        let index = java_protocol::target::ENTITY_TYPES
            .iter()
            .position(|name| *name == entity.kind.as_ref())?;
        f64::from(java_protocol::target::ENTITY_SPECS[index].dimensions[2] * entity.visual.scale)
    };
    position[1] += eye / 2.;
    Some(position)
}

pub(in crate::java) fn take(
    state: &mut super::super::WorldState,
    scope: (u64, u64),
    item: i32,
    item_lifetime: u64,
    collector: i32,
    collector_lifetime: u64,
    amount: i32,
) -> Result<bool, &'static str> {
    let Some(entity) = state.entities.get(item).filter(|entity| {
        item_lifetime != 0
            && entity.transport_lifetime == item_lifetime
            && entity.kind.as_ref() == java_protocol::target::ENTITY_ITEM
    }) else {
        return Ok(false);
    };
    let Some(ground) = state.equipment.ground(entity.identity).cloned() else {
        return Ok(false);
    };
    let own = state
        .bootstrap
        .as_ref()
        .ok_or("Java pickup before player identity")?
        .world
        .entity_id;
    let collector = if collector == own {
        state
            .player_motion
            .map(|motion| (Collector::Local(collector), motion.position))
    } else {
        state
            .entities
            .get(collector)
            .filter(|entity| {
                collector_lifetime != 0 && entity.transport_lifetime == collector_lifetime
            })
            .and_then(|entity| {
                target(entity).map(|target| {
                    (
                        Collector::Remote {
                            network_id: collector,
                            identity: entity.identity,
                        },
                        target,
                    )
                })
            })
    };
    let source = entity.identity;
    let source_revision = entity.revision;
    let uuid = entity.uuid;
    let origin = entity
        .item_motion
        .as_ref()
        .filter(|view| view.active)
        .map_or(entity.motion.position, |view| view.position);
    // Listener extracts the immutable render state at partial tick 1 before shrinking.
    let age = entity.visual_ticks as f32 + 1.;
    if let Some((collector, target)) = collector {
        let identity = state.entities.presentation_identity(scope)?;
        state.equipment.pickups.push(Pickup {
            identity,
            source,
            source_network_id: item,
            uuid,
            revision: source_revision,
            collector,
            ground: ground.clone(),
            flight: item_motion::Pickup::new(origin, target),
            age,
            pending_local_target: matches!(collector, Collector::Local(_)),
            source_depleted: false,
        });
    }
    // ClientPacketListener snapshots before ItemStack.shrink. This changes only the visible
    // ground stack; it never inserts an item into the player's transactional inventory.
    let stack = ground.stack.and_then(|mut stack| {
        let count = (stack.count as i32).wrapping_sub(amount);
        stack.count = count.max(0) as u32;
        (stack.count > 0).then_some(stack)
    });
    state.equipment.set_ground(source, stack.clone())?;
    state.equipment.revision = state
        .equipment
        .revision
        .checked_add(1)
        .ok_or("Java pickup revision exhausted")?;
    if stack.is_none() {
        for pickup in &mut state.equipment.pickups {
            if pickup.source == source {
                pickup.source_depleted = true;
            }
        }
        state.entities.remove(scope, &[item])?;
        state.equipment.ground.remove(&source);
    }
    Ok(true)
}
