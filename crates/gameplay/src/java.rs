//! Native Java movement orchestration. WorldState retains the only predicted player motion.
pub mod actions;
pub mod attack;
pub mod interaction;
pub mod item_use;
pub mod mining;
mod sprint;
pub mod view;
use assets::registry::java::physics::Table;
use client_world::java::WorldState;
use sim::java::travel::{self, Body, Input, Parameters};
use std::time::Duration;
use world::java::{
    dynamics::{Attribute, Modifier, Operation},
    player::Motion,
};

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
    pub status_changed: bool,
    pub horizontal_collision: bool,
    pub input: Option<u8>,
    pub sprinting: Option<bool>,
}
#[derive(Default)]
pub struct Controller {
    scope: Option<(u64, u64)>,
    sequence: u64,
    pub body: Body,
    sprint: sprint::Trigger,
    view: view::History,
    pub use_effects: Option<item_use::plan::Effects>,
    elapsed: Duration,
    pending: Option<Frame>,
    last_position: Option<[f64; 3]>,
    last_rotation: Option<[f32; 2]>,
    last_input: Option<u8>,
    last_sprint: Option<bool>,
    last_flags: Option<(bool, bool)>,
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
            self.last_flags = None;
            self.last_rotation = state.player_motion.map(|m| [m.yaw, m.pitch]);
            self.reminder = 0;
            self.view = Default::default();
            if let Some(motion) = state.player_motion {
                self.view.reset(view::Sample {
                    feet: motion.position,
                    eye_height: self.eye_height(state),
                });
            }
            // The pinned position handler updates kinematics, not entity pose/fluid history.
            // A new session/dimension retires Body above; same-world corrections retain it.
        }
        if state.awaiting_correction_ack() {
            self.elapsed = Duration::ZERO;
        } else {
            self.elapsed = self.elapsed.saturating_add(elapsed);
        }
        corrected
    }

    /// Apply an admitted block-prediction rollback only when its restored shape intersects us.
    /// The caller owns ACK ordering, saved-position admission and later-teleport suppression.
    pub fn reconcile_block(
        &mut self,
        state: &mut WorldState,
        physics: &Table,
        cell: [i32; 3],
        restored: u32,
        saved: [f64; 3],
    ) -> Result<bool, String> {
        let scope = state.terrain.dimension().map(|d| (d.session, d.epoch));
        if scope.is_none() || self.scope != scope {
            return Err("Block rollback requires the current movement scope".into());
        }
        let Some(mut motion) = state.player_motion else {
            return Ok(false);
        };
        let scale = state
            .player_dynamics
            .attributes
            .get("minecraft:scale")
            .ok_or("Block rollback awaits the native scale attribute")?
            .value()
            .map_err(str::to_owned)? as f32;
        if !saved.into_iter().all(f64::is_finite) || !scale.is_finite() || scale <= 0. {
            return Err("Invalid block rollback position or scale".into());
        }
        if !sim::java::collision::restored_block_collides(
            &state.terrain,
            physics,
            cell,
            restored,
            self.body.bounds(motion.position, scale),
            sim::java::collision::PlayerContext {
                bottom: motion.position[1],
                descending: self.body.descending(),
            },
        )
        .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        // Entity.absSnapTo clamps horizontal coordinates and retains current kinematics.
        motion.position = [
            saved[0].clamp(-3.0e7, 3.0e7),
            saved[1],
            saved[2].clamp(-3.0e7, 3.0e7),
        ];
        state.player_motion = Some(motion);
        self.pending = None;
        self.view.reset(view::Sample {
            feet: motion.position,
            eye_height: self.body.eye_height(scale),
        });
        Ok(true)
    }

    /// Input authority release retires both pending taps and sprint continuation.
    pub fn release_input(&mut self) {
        self.sprint.release();
    }

    /// Render-only sample; authoritative picking and movement continue using WorldState.
    pub fn view_sample(&self, state: &WorldState) -> Option<view::Sample> {
        if self.scope != state.terrain.dimension().map(|d| (d.session, d.epoch)) {
            return state.player_motion.map(|m| view::Sample {
                feet: m.position,
                eye_height: self.eye_height(state),
            });
        }
        self.view
            .sample(self.elapsed.as_secs_f64() / TICK.as_secs_f64())
            .or_else(|| {
                state.player_motion.map(|m| view::Sample {
                    feet: m.position,
                    eye_height: self.eye_height(state),
                })
            })
    }

    /// Effective native attribute, including exactly one predicted sprint modifier.
    pub fn view_speed(&self, state: &WorldState) -> Option<f32> {
        let (walking, sprinting) = movement_speeds(state).ok()?;
        Some(if self.body.sprinting {
            sprinting
        } else {
            walking
        })
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
        let scope = state.terrain.dimension().map(|d| (d.session, d.epoch));
        let body = if scope.is_some() && self.scope == scope {
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
        self.last_flags = Some((frame.on_ground, frame.horizontal_collision));
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

    /// LocalPlayer.sendPosition compares against the last frame admitted to transport.
    fn next_frame(
        &self,
        epoch: u64,
        entity: i32,
        motion: Motion,
        body: Body,
        buttons: u8,
    ) -> Frame {
        let moved = self.last_position.is_none_or(|old| {
            old.into_iter()
                .zip(motion.position)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
                > 0.0002_f64.powi(2)
        }) || self.reminder.saturating_add(1) >= 20;
        let rotation = [motion.yaw, motion.pitch];
        Frame {
            dimension_epoch: epoch,
            sequence: self.sequence,
            entity,
            position: moved.then_some(motion.position),
            rotation: (self.last_rotation != Some(rotation)).then_some(rotation),
            on_ground: body.on_ground,
            horizontal_collision: body.horizontal_collision,
            status_changed: self.last_flags != Some((body.on_ground, body.horizontal_collision)),
            input: (self.last_input != Some(buttons)).then_some(buttons),
            sprinting: (self.last_sprint != Some(body.sprinting)).then_some(body.sprinting),
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
        let possible = (abilities.may_fly || state.vitals.as_ref().is_some_and(|v| v.food > 6))
            && !state
                .player_dynamics
                .effects
                .contains_key("minecraft:blindness");
        let slow = self.use_effects.is_some_and(|e| !e.can_sprint);
        let can_start = (!input.crouch && !self.body.low_profile || self.body.eyes_in_water())
            && (self.body.water_height <= 0. || self.body.eyes_in_water());
        let mut sprint = self.sprint;
        input.sprint = sprint.select(
            input,
            self.body.sprinting,
            possible,
            can_start,
            slow,
            controls.buttons & 2 != 0,
        );
        let mut parameters = parameters(state)?;
        if let Some(effects) = self.use_effects {
            input.sprint &= effects.can_sprint;
            parameters.use_speed_multiplier = effects.speed_multiplier;
        }
        motion.yaw = controls.yaw;
        motion.pitch = controls.pitch;
        let mut body = self.body;
        let before = view::Sample {
            feet: motion.position,
            eye_height: self.eye_height(state),
        };
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
        self.pending = Some(self.next_frame(
            scope.1,
            bootstrap.world.entity_id,
            motion,
            body,
            controls.buttons,
        ));
        self.reminder = self.reminder.saturating_add(1);
        self.body = body;
        self.view.tick(
            before,
            view::Sample {
                feet: motion.position,
                eye_height: self.eye_height(state),
            },
        );
        self.sprint = sprint;
        state.player_motion = Some(motion);
        state.player_pose_revision = revision;
        Ok(true)
    }
}

fn parameters(state: &WorldState) -> Result<Parameters<'_>, String> {
    let get = |name: &str| -> Result<&Attribute, String> {
        state
            .player_dynamics
            .attributes
            .get(name)
            .ok_or_else(|| format!("Missing pinned Java player attribute {name}"))
    };
    let value = |name| -> Result<f64, String> { get(name)?.value().map_err(str::to_owned) };
    let (base_speed, sprint_speed) = movement_speeds(state)?;
    let climbable = state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.tags.get("minecraft:block"))
        .and_then(|tags| tags.get("minecraft:climbable"))
        .map_or(&[][..], Vec::as_slice);
    let suppresses_bounce = state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.tags.get("minecraft:block"))
        .and_then(|tags| {
            tags.get(assets::registry::java::physics::callbacks::suppresses_bounce_tag())
        })
        .map_or(&[][..], Vec::as_slice);
    Ok(Parameters {
        speed: base_speed,
        sprint_speed,
        friction_modifier: value("minecraft:friction_modifier")? as f32,
        air_drag_modifier: value("minecraft:air_drag_modifier")? as f32,
        gravity: value("minecraft:gravity")?,
        bounciness: value("minecraft:bounciness")?,
        weaving: state
            .player_dynamics
            .effects
            .contains_key("minecraft:weaving"),
        jump_strength: value("minecraft:jump_strength")? as f32,
        step_height: value("minecraft:step_height")? as f32,
        scale: value("minecraft:scale")? as f32,
        use_speed_multiplier: 1.,
        sneaking_speed: value("minecraft:sneaking_speed")? as f32,
        movement_efficiency: value("minecraft:movement_efficiency")? as f32,
        water_movement_efficiency: value("minecraft:water_movement_efficiency")? as f32,
        may_fly: state
            .player_dynamics
            .abilities
            .as_ref()
            .is_some_and(|a| a.may_fly),
        climbable,
        suppresses_bounce,
        fast_lava: state
            .fast_lava()?
            .unwrap_or_else(assets::registry::java::fluids::contacts::fast_lava_default),
    })
}

#[cfg(test)]
mod tests;

fn movement_speeds(state: &WorldState) -> Result<(f32, f32), String> {
    let mut speed = state
        .player_dynamics
        .attributes
        .get("minecraft:movement_speed")
        .ok_or("Missing pinned Java movement speed")?
        .clone();
    speed.modifiers.retain(|m| m.name != "minecraft:sprinting");
    let base_speed = speed.value().map_err(str::to_owned)? as f32;
    speed.modifiers.push(Modifier {
        name: "minecraft:sprinting".into(),
        amount: f64::from(0.3_f32),
        operation: Operation::AddMultipliedTotal,
    });
    Ok((base_speed, speed.value().map_err(str::to_owned)? as f32))
}
