//! Native entity authority, independent of ECS and packet-library types.
use super::player::{Motion, Relative};
pub mod animation;
pub mod interpolation;
pub mod visual;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity {
    pub session: u64,
    pub epoch: u64,
    pub incarnation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionStep {
    pub position: [f64; 3],
    pub tick_offset: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passenger {
    LocalPlayer(i32),
    Entity { network_id: i32, identity: Identity },
}
impl Passenger {
    fn network_id(self) -> i32 {
        match self {
            Self::LocalPlayer(id) | Self::Entity { network_id: id, .. } => id,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Passengers {
    pub vehicle: Identity,
    pub members: Vec<Passenger>,
}

#[derive(Clone, Debug)]
pub struct Spawn {
    pub network_id: i32,
    pub uuid: [u8; 16],
    pub kind: String,
    pub motion: Motion,
    pub head_yaw: f32,
    pub object_data: i32,
    pub visual: visual::Visual,
    /// Present only for an entity presentation schema implemented by the native consumer.
    pub interpolation_interval: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub identity: Identity,
    pub network_id: i32,
    pub uuid: [u8; 16],
    pub kind: Arc<str>,
    pub motion: Motion,
    /// Native delta decoding has its own anchor; presentation never overwrites it.
    pub delta_base: [f64; 3],
    pub head_yaw: f32,
    pub object_data: i32,
    pub on_ground: bool,
    pub revision: u64,
    /// Independent of ordinary updates so coalesced rotations cannot hide a teleport.
    pub teleport_revision: u64,
    /// Ordered intermediate positions; each tick offset is measured from the preceding step.
    pub path: Arc<[PositionStep]>,
    pub visual: visual::Visual,
    pub presentation: Option<animation::Presentation>,
}

#[derive(Clone, Debug, Default)]
pub struct Entities {
    scope: Option<(u64, u64)>,
    entries: BTreeMap<i32, Entity>,
    passengers: BTreeMap<i32, Passengers>,
    next_incarnation: u64,
    revision: u64,
    pub unknown_updates: u64,
}
impl Entities {
    pub fn begin(&mut self, session: u64, epoch: u64) {
        *self = Self {
            scope: Some((session, epoch)),
            ..Default::default()
        };
    }
    pub fn accepts(&self, session: u64, epoch: u64) -> bool {
        self.scope == Some((session, epoch))
    }
    pub fn get(&self, network_id: i32) -> Option<&Entity> {
        self.entries.get(&network_id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.entries.values()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Advances cosmetic native tracks without changing authoritative motion or delta anchors.
    pub fn tick_presentation(&mut self) {
        let riders = self
            .passengers
            .values()
            .flat_map(|p| p.members.iter())
            .filter_map(|p| match p {
                Passenger::Entity { identity, .. } => Some(*identity),
                Passenger::LocalPlayer(_) => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        for entity in self.entries.values_mut() {
            if let Some(presentation) = &mut entity.presentation {
                presentation.tick(
                    entity.visual.baby,
                    entity.visual.health.is_none_or(|h| h > 0.),
                    riders.contains(&entity.identity),
                    self.passengers
                        .get(&entity.network_id)
                        .is_some_and(|p| !p.members.is_empty()),
                );
            }
        }
    }
    pub fn pending_presentation_steps(&self) -> usize {
        self.entries
            .values()
            .filter_map(|e| e.presentation.as_ref())
            .map(|p| p.track.pending_steps())
            .sum()
    }
    pub fn passengers(&self, vehicle: i32) -> Option<&Passengers> {
        self.passengers.get(&vehicle)
    }
    pub fn local_vehicle(&self) -> Option<Identity> {
        self.passengers
            .values()
            .find(|p| {
                p.members
                    .iter()
                    .any(|m| matches!(m, Passenger::LocalPlayer(_)))
            })
            .map(|p| p.vehicle)
    }

    /// References bind to current lifetimes; unknown IDs do not attach to a future reused ID.
    pub fn set_passengers(
        &mut self,
        scope: (u64, u64),
        vehicle: i32,
        ids: &[i32],
        local_player: i32,
    ) -> Result<bool, &'static str> {
        if !self.accepts(scope.0, scope.1) {
            return Ok(false);
        }
        let Some(owner) = self.entries.get(&vehicle) else {
            self.unknown_updates = self.unknown_updates.saturating_add(1);
            return Ok(false);
        };
        let identity = owner.identity;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Java entity revision overflow")?;
        let mut seen = std::collections::BTreeSet::new();
        let mut members = Vec::new();
        for &id in ids {
            if id == vehicle || !seen.insert(id) || self.ancestor_of(id, vehicle) {
                continue;
            }
            if id == local_player {
                members.push(Passenger::LocalPlayer(id));
            } else if let Some(e) = self.entries.get(&id) {
                members.push(Passenger::Entity {
                    network_id: id,
                    identity: e.identity,
                });
            } else {
                self.unknown_updates = self.unknown_updates.saturating_add(1);
            }
        }
        // Starting a new ride detaches each passenger from its previous vehicle.
        for (id, p) in &mut self.passengers {
            if *id != vehicle {
                p.members.retain(|old| !members.contains(old));
            }
        }
        self.passengers.insert(
            vehicle,
            Passengers {
                vehicle: identity,
                members,
            },
        );
        self.revision = revision;
        Ok(true)
    }

    fn ancestor_of(&self, candidate: i32, mut child: i32) -> bool {
        for _ in 0..self.entries.len() {
            let Some((&parent, _)) = self
                .passengers
                .iter()
                .find(|(_, p)| p.members.iter().any(|m| m.network_id() == child))
            else {
                return false;
            };
            if parent == candidate {
                return true;
            }
            child = parent;
        }
        false
    }
    fn detach(&mut self, id: i32) {
        self.passengers.remove(&id);
        for p in self.passengers.values_mut() {
            p.members.retain(|m| m.network_id() != id);
        }
    }

    /// A repeated network ID replaces its prior lifetime, including every presentation path.
    pub fn spawn(&mut self, scope: (u64, u64), spawn: Spawn) -> Result<bool, &'static str> {
        if !self.accepts(scope.0, scope.1) {
            return Ok(false);
        }
        if !spawn.motion.is_finite()
            || !spawn.head_yaw.is_finite()
            || spawn.visual.health.is_some_and(|h| !h.is_finite())
        {
            return Err("nonfinite native entity spawn");
        }
        let incarnation = self
            .next_incarnation
            .checked_add(1)
            .ok_or("Java entity incarnation overflow")?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Java entity revision overflow")?;
        let presentation = spawn
            .interpolation_interval
            .map(|interval| {
                animation::Presentation::new(
                    spawn.motion.into(),
                    spawn.head_yaw,
                    interval,
                    if spawn.visual.player.is_some() {
                        animation::BodyKind::Player
                    } else {
                        animation::BodyKind::Pig
                    },
                )
            })
            .transpose()?;
        self.detach(spawn.network_id);
        self.entries.insert(
            spawn.network_id,
            Entity {
                identity: Identity {
                    session: scope.0,
                    epoch: scope.1,
                    incarnation,
                },
                network_id: spawn.network_id,
                uuid: spawn.uuid,
                kind: spawn.kind.into(),
                motion: spawn.motion,
                delta_base: spawn.motion.position,
                head_yaw: spawn.head_yaw,
                object_data: spawn.object_data,
                on_ground: false,
                revision,
                teleport_revision: 1,
                path: Arc::from([]),
                visual: spawn.visual,
                presentation,
            },
        );
        self.next_incarnation = incarnation;
        self.revision = revision;
        Ok(true)
    }

    pub fn remove(&mut self, scope: (u64, u64), ids: &[i32]) -> Result<bool, &'static str> {
        if !self.accepts(scope.0, scope.1) {
            return Ok(false);
        }
        if !ids.iter().any(|id| self.entries.contains_key(id)) {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Java entity revision overflow")?;
        for id in ids {
            self.entries.remove(id);
            self.detach(*id);
        }
        self.revision = revision;
        Ok(true)
    }

    /// Applies a complete update atomically; unknown IDs never create phantom entities.
    pub fn update(
        &mut self,
        scope: (u64, u64),
        id: i32,
        update: impl FnOnce(&mut Entity) -> Result<(), &'static str>,
    ) -> Result<bool, &'static str> {
        if !self.accepts(scope.0, scope.1) {
            return Ok(false);
        }
        let Some(old) = self.entries.get(&id) else {
            self.unknown_updates = self.unknown_updates.saturating_add(1);
            return Ok(false);
        };
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Java entity revision overflow")?;
        let mut next = old.clone();
        update(&mut next)?;
        if next.identity != old.identity
            || next.network_id != old.network_id
            || next.uuid != old.uuid
            || next.kind != old.kind
            || next.object_data != old.object_data
        {
            return Err("native entity lifetime changes require a new spawn");
        }
        if next.visual.health.is_some_and(|health| !health.is_finite())
            || !next.motion.is_finite()
            || !next.head_yaw.is_finite()
            || !next.delta_base.into_iter().all(f64::is_finite)
            || !next
                .path
                .iter()
                .flat_map(|s| s.position)
                .all(f64::is_finite)
        {
            return Err("nonfinite native entity update");
        }
        next.revision = revision;
        if let Some(presentation) = &next.presentation {
            let previous = old
                .presentation
                .as_ref()
                .map_or(0, |p| p.track.pending_steps());
            if previous != presentation.track.pending_steps()
                && self.pending_presentation_steps() - previous + presentation.track.pending_steps()
                    > interpolation::MAX_PENDING_STEPS
            {
                return Err("native interpolation exceeds provisional resident work budget");
            }
        }
        self.entries.insert(id, next);
        self.revision = revision;
        Ok(true)
    }
}
impl Entity {
    /// Called for each decoded position/rotation packet, including unchanged targets.
    /// Metadata/velocity updates must not restart this cosmetic timeline.
    pub fn queue_presentation_motion(&mut self) -> Result<(), &'static str> {
        if let Some(presentation) = &mut self.presentation {
            presentation.retarget(self.motion.into(), &self.path)?;
        }
        Ok(())
    }
    pub fn queue_presentation_head(&mut self) {
        if let Some(presentation) = &mut self.presentation {
            presentation.head_to(self.head_yaw);
        }
    }
    /// Absolute synchronization compares the displayed entity position to the new wire target.
    pub fn synchronize_presentation_motion(&mut self) -> Result<(), &'static str> {
        if let Some(presentation) = &mut self.presentation {
            if interpolation::within_correction_distance(
                presentation.track.current.position,
                self.motion.position,
            ) {
                presentation.retarget(self.motion.into(), &self.path)?;
            } else {
                presentation.snap(self.motion.into());
            }
        }
        Ok(())
    }
    /// Native teleport uses the pending target when interpolating, otherwise the completed pose.
    /// Unlike absolute synchronization it never rebases the independent delta-packet decoder.
    pub fn teleport(
        &mut self,
        change: Motion,
        relative: Relative,
        on_ground: bool,
    ) -> Result<(), &'static str> {
        let mut base = self.motion;
        if let Some(presentation) = &self.presentation
            && presentation.track.pending_steps() == 0
        {
            let pose = presentation.track.current;
            base.position = pose.position;
            base.yaw = pose.yaw;
            base.pitch = pose.pitch;
        }
        let motion = base.corrected(change, relative)?;
        let revision = self
            .teleport_revision
            .checked_add(1)
            .ok_or("Java entity teleport revision overflow")?;
        if let Some(presentation) = &mut self.presentation {
            if interpolation::within_correction_distance(base.position, motion.position) {
                presentation.retarget(motion.into(), &[])?;
            } else {
                presentation.snap(motion.into());
            }
        }
        self.motion = motion;
        self.path = Arc::from([]);
        self.on_ground = on_ground;
        self.teleport_revision = revision;
        Ok(())
    }
}

/// Java VecDeltaCodec preserves an unquantized axis for a zero delta. Other axes use
/// Math.round(base * 4096), whose ties go toward positive infinity, including negative ties.
pub fn decode_delta(base: [f64; 3], delta: [i16; 3]) -> [f64; 3] {
    std::array::from_fn(|i| {
        if delta[i] == 0 {
            base[i]
        } else {
            java_round(base[i] * 4096.).wrapping_add(i64::from(delta[i])) as f64 / 4096.
        }
    })
}

/// Binary rounding keeps Java's tie/saturation behavior without an inexact `x + 0.5`.
fn java_round(value: f64) -> i64 {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let shift = 1074 - exponent;
    if (0..64).contains(&shift) {
        let mantissa = ((bits & ((1 << 52) - 1)) | (1 << 52)) as i64;
        let signed = if bits >> 63 != 0 { -mantissa } else { mantissa };
        ((signed >> shift) + 1) >> 1
    } else {
        value as i64
    }
}

#[cfg(test)]
mod tests;
