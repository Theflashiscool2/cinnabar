//! Native Java movement orchestration. WorldState retains the only predicted player motion.
use assets::registry::java::physics::Table;
use client_world::java::WorldState;
use sim::java::travel::{self, Body, Input, Parameters};
use std::time::Duration;
use world::java::dynamics::{Attribute, Modifier, Operation};

const TICK: Duration = Duration::from_nanos(1_000_000_000 / travel::TICKS_PER_SECOND as u64);

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub travel: Input,
    pub buttons: u8,
    pub yaw: f32,
    pub pitch: f32,
}
#[derive(Clone, Debug)]
pub struct Frame {
    pub dimension_epoch: u64,
    pub sequence: u64,
    pub entity: i32,
    pub position: Option<[f64; 3]>,
    pub rotation: Option<[f32; 2]>,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub input: Option<u8>,
    pub sprinting: Option<bool>,
}
#[derive(Default)]
pub struct Controller {
    scope: Option<(u64, u64)>,
    sequence: u64,
    teleport: Option<u64>,
    pub body: Body,
    elapsed: Duration,
    pending: Option<Frame>,
    last_position: Option<[f64; 3]>,
    last_rotation: Option<[f32; 2]>,
    last_input: Option<u8>,
    last_sprint: Option<bool>,
    reminder: u8,
    pub ticks: u64,
    pub blocked: Option<String>,
}
impl Controller {
    /// Invalidates buffered movement on server corrections. A true result requires the caller
    /// to replace its camera angles from admitted motion before sampling movement controls.
    pub fn begin_frame(&mut self, state: &WorldState, elapsed: Duration) -> bool {
        let scope = state.terrain.dimension().map(|d| (d.session, d.epoch));
        if self.scope != scope {
            *self = Self {
                scope,
                ..Default::default()
            };
        }
        let corrected = self.sequence != state.motion_sequence();
        if corrected {
            self.sequence = state.motion_sequence();
            self.pending = None;
            self.elapsed = Duration::ZERO;
            // A buffered old receipt may have been discarded by the socket owner. Re-send
            // pose and input intent so queued baselines cannot hide an unsent update.
            self.last_position = None;
            self.last_input = None;
            self.last_sprint = None;
            self.last_rotation = state.player_motion.map(|m| [m.yaw, m.pitch]);
            self.reminder = 0;
            let teleport = state.teleport.map(|t| t.revision);
            if self.teleport != teleport {
                self.body = Body::default();
                self.teleport = teleport;
            }
        }
        if state.awaiting_correction_ack() {
            self.elapsed = Duration::ZERO;
        } else {
            self.elapsed = self.elapsed.saturating_add(elapsed);
        }
        corrected
    }

    pub fn pending(&self) -> Option<&Frame> {
        self.pending.as_ref()
    }
    pub fn backlog_ticks(&self) -> u128 {
        self.elapsed.as_nanos() / TICK.as_nanos()
    }

    pub fn eye_height(&self, state: &WorldState) -> f64 {
        let scale = state
            .player_dynamics
            .attributes
            .get("minecraft:scale")
            .and_then(|a| a.value().ok())
            .unwrap_or(1.) as f32;
        let body = if self.teleport == state.teleport.map(|t| t.revision) {
            self.body
        } else {
            Body::default()
        };
        body.eye_height(scale)
    }

    /// Commits the outbound baseline only after the bounded transport queue accepts the batch.
    pub fn queued(&mut self) {
        let Some(frame) = self.pending.take() else {
            return;
        };
        if let Some(position) = frame.position {
            self.last_position = Some(position);
            self.reminder = 0;
        }
        if let Some(rotation) = frame.rotation {
            self.last_rotation = Some(rotation);
        }
        if let Some(input) = frame.input {
            self.last_input = Some(input);
        }
        if let Some(sprinting) = frame.sprinting {
            self.last_sprint = Some(sprinting);
        }
    }

    /// Prepares at most one ordered tick; a full queue retains it and prevents further prediction.
    pub fn tick(&mut self, state: &mut WorldState, physics: &Table, controls: Controls) -> bool {
        if self.pending.is_some() || self.elapsed < TICK || state.awaiting_correction_ack() {
            return false;
        }
        match self.prepare(state, physics, controls) {
            Ok(true) => {
                self.elapsed -= TICK;
                self.ticks = self.ticks.saturating_add(1);
                self.blocked = None;
                true
            }
            Ok(false) => {
                self.elapsed = Duration::ZERO;
                false
            }
            Err(error) => {
                self.blocked = Some(error);
                self.elapsed = Duration::ZERO;
                false
            }
        }
    }

    fn prepare(
        &mut self,
        state: &mut WorldState,
        physics: &Table,
        controls: Controls,
    ) -> Result<bool, String> {
        let Some(scope) = self.scope else {
            return Ok(false);
        };
        let Some(mut motion) = state.player_motion else {
            return Ok(false);
        };
        let Some(bootstrap) = state.bootstrap.as_ref() else {
            return Ok(false);
        };
        let Some(abilities) = state.player_dynamics.abilities.as_ref() else {
            return Ok(false);
        };
        if state.death.is_some() || state.vitals.as_ref().is_some_and(|v| v.health <= 0.) {
            return Ok(false);
        }
        if abilities.flying {
            return Err("Native Java flying movement is not implemented".into());
        }
        if state.entities.local_vehicle().is_some() {
            return Err("Native Java mounted movement is not implemented".into());
        }
        let mut input = controls.travel;
        input.sprint &= !input.crouch
            && input.forward > 0.8
            && !self.body.horizontal_collision
            && (abilities.may_fly || state.vitals.as_ref().is_some_and(|v| v.food > 6));
        let parameters = parameters(state)?;
        motion.yaw = controls.yaw;
        motion.pitch = controls.pitch;
        let mut body = self.body;
        travel::step(
            &mut motion,
            &mut body,
            input,
            parameters,
            &state.terrain,
            physics,
            scope,
        )
        .map_err(|e| e.to_string())?;
        let revision = state
            .player_pose_revision
            .checked_add(1)
            .ok_or("Java player pose revision overflow")?;
        let next_reminder = self.reminder.saturating_add(1);
        let moved = self.last_position.is_none_or(|old| {
            old.into_iter()
                .zip(motion.position)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
                > 0.0002_f64.powi(2)
        }) || next_reminder >= 20;
        let rotation = [motion.yaw, motion.pitch];
        self.pending = Some(Frame {
            dimension_epoch: scope.1,
            sequence: self.sequence,
            entity: bootstrap.world.entity_id,
            position: moved.then_some(motion.position),
            rotation: (self.last_rotation != Some(rotation)).then_some(rotation),
            on_ground: body.on_ground,
            horizontal_collision: body.horizontal_collision,
            input: (self.last_input != Some(controls.buttons)).then_some(controls.buttons),
            sprinting: (self.last_sprint != Some(body.sprinting)).then_some(body.sprinting),
        });
        self.reminder = next_reminder;
        self.body = body;
        state.player_motion = Some(motion);
        state.player_pose_revision = revision;
        Ok(true)
    }
}

fn parameters(state: &WorldState) -> Result<Parameters, String> {
    let get = |name: &str| -> Result<&Attribute, String> {
        state
            .player_dynamics
            .attributes
            .get(name)
            .ok_or_else(|| format!("Missing pinned Java player attribute {name}"))
    };
    let value = |name| -> Result<f64, String> { get(name)?.value().map_err(str::to_owned) };
    if value("minecraft:bounciness")? != 0. {
        return Err("Native Java attribute-driven bouncing is not implemented".into());
    }
    let mut speed = get("minecraft:movement_speed")?.clone();
    speed.modifiers.retain(|m| m.name != "minecraft:sprinting");
    let base_speed = speed.value().map_err(str::to_owned)? as f32;
    speed.modifiers.push(Modifier {
        name: "minecraft:sprinting".into(),
        amount: f64::from(0.3_f32),
        operation: Operation::AddMultipliedTotal,
    });
    Ok(Parameters {
        speed: base_speed,
        sprint_speed: speed.value().map_err(str::to_owned)? as f32,
        friction_modifier: value("minecraft:friction_modifier")? as f32,
        air_drag_modifier: value("minecraft:air_drag_modifier")? as f32,
        gravity: value("minecraft:gravity")?,
        jump_strength: value("minecraft:jump_strength")? as f32,
        step_height: value("minecraft:step_height")? as f32,
        scale: value("minecraft:scale")? as f32,
        sneaking_speed: value("minecraft:sneaking_speed")? as f32,
        movement_efficiency: value("minecraft:movement_efficiency")? as f32,
    })
}

#[cfg(test)]
mod tests;
