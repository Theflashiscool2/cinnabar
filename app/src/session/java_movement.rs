//! Binds native Java input/physics to the existing session and camera owners.
use super::{SessionController, java_render::Presentation};
use bevy::{prelude::*, time::Real};
use client_presentation::local_player::LocalViewPose;
use client_state::SemanticInputSnapshot;
use java_protocol::session::{Command, movement::Frame};
use semantic_input::Action;

#[derive(Resource, Default)]
pub(crate) struct Movement(pub gameplay::java::Controller);

impl Movement {
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        serde_json::json!({"ticks":self.0.ticks,"blocked":self.0.blocked,"backlog_ticks":self.0.backlog_ticks(),
            "pending_frame":self.0.pending().is_some(),"on_ground":self.0.body.on_ground,
            "horizontal_collision":self.0.body.horizontal_collision,"crouched":self.0.body.crouched,"sprinting":self.0.body.sprinting})
    }
}

pub(crate) fn update(
    time: Res<Time<Real>>,
    input: Res<SemanticInputSnapshot>,
    mut view: ResMut<LocalViewPose>,
    controller: Option<ResMut<SessionController>>,
    presentation: Res<Presentation>,
    mut movement: ResMut<Movement>,
) {
    let Some(mut controller) = controller else {
        return;
    };
    let SessionController {
        java, java_world, ..
    } = &mut *controller;
    let Some(java) = java.as_ref() else {
        movement.0 = Default::default();
        return;
    };
    let physics = presentation.movement_physics();
    let corrected = movement.0.begin_frame(
        java_world,
        if physics.is_some() {
            time.delta()
        } else {
            std::time::Duration::ZERO
        },
    );
    // World publication follows network send. Without this replacement a simulation tick
    // could overwrite an admitted correction with the previous frame's camera rotation.
    if corrected && let Some(motion) = java_world.player_motion {
        view.set_rotation(
            Quat::from_rotation_y((180. - motion.yaw).to_radians())
                * Quat::from_rotation_x(-motion.pitch.to_radians()),
        );
        view.reanchor_camera();
    }
    let Some(physics) = physics else {
        return;
    };
    let axes = input.0.raw_movement();
    let held = |action| input.0.phase(action).held;
    let buttons = input
        .0
        .snapshot()
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
    let (yaw, pitch, _) = view.rotation().to_euler(EulerRot::YXZ);
    let controls = gameplay::java::Controls {
        travel: sim::java::travel::Input {
            sideways: -axes[0],
            forward: axes[1],
            jump: held(Action::Jump),
            crouch: held(Action::Sneak),
            sprint: held(Action::Sprint),
        },
        buttons: bits,
        yaw: 180. - yaw.to_degrees(),
        pitch: -pitch.to_degrees(),
    };
    // Four ticks bound per-frame work; unconsumed elapsed time remains visible in diagnostics.
    for _ in 0..4 {
        if movement.0.pending().is_none() && !movement.0.tick(java_world, physics, controls) {
            break;
        }
        let Some(frame) = movement.0.pending() else {
            break;
        };
        let command = Command::Move {
            dimension_epoch: frame.dimension_epoch,
            sequence: frame.sequence,
            frame: Frame {
                entity: frame.entity,
                position: frame.position,
                rotation: frame.rotation,
                on_ground: frame.on_ground,
                horizontal_collision: frame.horizontal_collision,
                input: frame.input,
                sprinting: frame.sprinting,
            },
        };
        match java.command(command) {
            Ok(()) => movement.0.queued(),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => break,
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                movement.0.blocked = Some("Java movement transport closed".into());
                break;
            }
        }
    }
}
