//! Binds native Java input/physics to the existing session and camera owners.
use super::{SessionController, java_render::Presentation};
use bevy::prelude::*;
use client_presentation::local_player::LocalViewPose;
use client_state::SemanticInputSnapshot;
use java_protocol::session::{Command, movement::Frame};
use semantic_input::Action;

#[derive(Resource, Default)]
pub(crate) struct Movement(pub gameplay::java::Controller, pub(super) ServiceState);
mod clock;
mod reconciliation;
use clock::ServiceState;
impl Movement {
    pub(super) fn elapsed(
        &mut self,
        state: &client_world::java::WorldState,
        now: web_time::Instant,
    ) -> std::time::Duration {
        self.1
            .elapsed(state.terrain.dimension().map(|d| (d.session, d.epoch)), now)
    }
}

impl Movement {
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"ticks":self.0.ticks,"between_frame_ticks":self.1.ticks,"blocked":self.0.blocked,"backlog_ticks":self.0.backlog_ticks(),
            "pending_frame":self.0.pending().is_some(),"pending_ability_update":self.0.pending_ability().map(|(_, _, flying)| flying),"on_ground":self.0.body.on_ground,
            "last_climbable_position":self.0.body.last_climbable_position,
            "swimming":self.0.body.swimming,"low_profile":self.0.body.low_profile,
            "eye_height":self.0.body.eye_height(1.),
            "water_height":self.0.body.water_height,"lava_height":self.0.body.lava_height,
            "horizontal_collision":self.0.body.horizontal_collision,"crouched":self.0.body.crouched,"sprinting":self.0.body.sprinting})
    }
}

pub(crate) fn update(
    input: Res<SemanticInputSnapshot>,
    mut view: ResMut<LocalViewPose>,
    controller: Option<ResMut<SessionController>>,
    presentation: Res<Presentation>,
    mut movement: ResMut<Movement>,
    interaction: Res<super::java_interaction::Interaction>,
) {
    let Some(mut controller) = controller else {
        return;
    };
    let SessionController {
        java, java_world, ..
    } = &mut *controller;
    let Some(java) = java.as_ref() else {
        *movement = Default::default();
        return;
    };
    let physics = presentation.movement_physics();
    let elapsed = movement.elapsed(java_world, web_time::Instant::now());
    let corrected = movement.0.begin_frame(java_world, elapsed);
    let mut physics = physics;
    let reconciled = match physics {
        Some(table) => match movement.reconcile_blocks(java_world, table) {
            Ok(reconciled) => reconciled,
            Err(error) => {
                movement.0.blocked = Some(error);
                physics = None;
                false
            }
        },
        None => false,
    };
    let corrected = corrected | reconciled | std::mem::take(&mut movement.1.corrected);
    // World publication follows network send. Without this replacement a simulation tick
    // could overwrite an admitted correction with the previous frame's camera rotation.
    if corrected && let Some(motion) = java_world.player_motion {
        view.set_rotation(super::java_look::view_rotation([motion.yaw, motion.pitch]));
        view.reanchor_camera();
    }
    if !flush_abilities(java, &mut movement) {
        return;
    }
    let released = input.0.snapshot().is_some_and(|s| s.authority_released);
    if released {
        movement.0.release_input();
    }
    if interaction
        .0
        .pending()
        .is_some_and(|r| r.kind == gameplay::java::interaction::Kind::ReleaseUse)
    {
        return;
    }
    movement.0.use_effects = interaction.0.use_effects();
    let axes = if released {
        [0.; 2]
    } else {
        input.0.raw_movement()
    };
    let held = |action| !released && input.0.phase(action).held;
    let buttons = input
        .0
        .snapshot()
        .filter(|_| !released)
        .map(|s| s.movement_buttons)
        .unwrap_or_default();
    let bits = [
        buttons.forward || axes[1] > 0.,
        buttons.backward || axes[1] < 0.,
        buttons.left || axes[0] < 0.,
        buttons.right || axes[0] > 0.,
        held(Action::Jump),
        held(Action::Sneak),
        held(Action::Sprint),
    ]
    .into_iter()
    .enumerate()
    .fold(0, |bits, (index, held)| bits | (u8::from(held) << index));
    let [yaw, pitch] = super::java_look::native_angles(
        view.rotation(),
        java_world.player_motion.map(|m| [m.yaw, m.pitch]),
    );
    let controls = gameplay::java::Controls {
        travel: sim::java::travel::Input {
            sideways: -axes[0],
            forward: axes[1],
            jump: held(Action::Jump),
            crouch: held(Action::Sneak),
            sprint: held(Action::Sprint),
        },
        buttons: bits,
        yaw,
        pitch,
    };
    movement.1.controls = controls;
    advance(java, java_world, physics, &mut movement);
}

/// Transfer native ability edges independently of collision/correction frame readiness.
pub(super) fn flush_abilities(
    java: &client_session::java::Session,
    movement: &mut Movement,
) -> bool {
    while let Some((dimension_epoch, authority_revision, flying)) = movement.0.pending_ability() {
        match java.command(Command::Abilities {
            dimension_epoch,
            authority_revision,
            flying,
        }) {
            Ok(()) => movement.0.ability_queued(),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => return false,
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                movement.0.blocked = Some("Java ability transport closed".into());
                return false;
            }
        }
    }
    true
}

/// The frame and between-frame service share one simulation and outbound baseline owner.
pub(super) fn advance(
    java: &client_session::java::Session,
    java_world: &mut client_world::java::WorldState,
    physics: Option<&assets::registry::java::physics::Table>,
    movement: &mut Movement,
) {
    if !flush_abilities(java, movement) {
        return;
    }
    // Four ticks bound each service slice; elapsed backlog remains explicit.
    for _ in 0..4 {
        if movement.0.pending().is_none()
            && movement.0.pending_tick_end().is_none()
            && !movement
                .0
                .game_tick(java_world, physics, movement.1.controls)
        {
            break;
        }
        if !flush_abilities(java, movement) {
            return;
        }
        let command = if let Some(frame) = movement.0.pending() {
            Command::Move {
                dimension_epoch: frame.dimension_epoch,
                sequence: frame.sequence,
                frame: Frame {
                    entity: frame.entity,
                    position: frame.position,
                    rotation: frame.rotation,
                    on_ground: frame.on_ground,
                    status_changed: frame.status_changed,
                    horizontal_collision: frame.horizontal_collision,
                    input: frame.input,
                    sprinting: frame.sprinting,
                },
            }
        } else if let Some(dimension_epoch) = movement.0.pending_tick_end() {
            Command::ClientTickEnd { dimension_epoch }
        } else {
            break;
        };
        match java.command(command) {
            Ok(()) => movement.0.tick_queued(),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => break,
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                movement.0.blocked = Some("Java movement transport closed".into());
                break;
            }
        }
    }
    if java.telemetry().is_some() && movement.1.reported_block != movement.0.blocked {
        bevy::log::debug!(target:"java_session", reason=?movement.0.blocked,
            backlog_ticks=movement.0.backlog_ticks(), sequence=java_world.motion_sequence(),
            "Native Java movement admission changed");
        movement.1.reported_block = movement.0.blocked.clone();
    }
}
