//! Binds edition-owned block selection and intent to native transport; no Bedrock packets.
use super::{SessionController, java_movement::Movement, java_render::Presentation};
use bevy::{prelude::*, time::Real};
use client_presentation::local_player::LocalViewPose;
use client_state::SemanticInputSnapshot;
use gameplay::java::actions::{Controller as Actions, Intent};
use gameplay::java::interaction::Kind;
use java_protocol::session::{
    Command,
    interaction::{Action, Dig, Face, Hand, UseOn},
};
use sim::java::selection;

#[derive(Resource, Default)]
pub(crate) struct Interaction(pub gameplay::java::interaction::Controller, pub Actions);
impl Interaction {
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        let hit = |h: selection::Hit| serde_json::json!({"block":h.block,"state":h.state,"position":h.position,"face":format!("{:?}",h.face),"inside":h.inside});
        serde_json::json!({"selected":self.0.selected.map(hit),"last_queued":self.0.last_queued.map(|r|serde_json::json!({"kind":format!("{:?}",r.kind),"sequence":r.sequence,"epoch":r.dimension_epoch,"hit":hit(r.hit)})),"queued_actions":self.0.queued_actions,"pending":self.0.pending().is_some(),"blocked":self.0.blocked,"backlog_ticks":self.0.backlog_ticks(),"prediction":false,"scope":"creative_breaking_and_main_hand_block_use", "input_backlog":self.1.backlog(), "input_blocked":self.1.blocked})
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "host binds session, input, camera and native asset owners"
)]
pub(crate) fn update(
    time: Res<Time<Real>>,
    input: Res<SemanticInputSnapshot>,
    view: Res<LocalViewPose>,
    controller: Option<ResMut<SessionController>>,
    presentation: Res<Presentation>,
    movement: Res<Movement>,
    mut interaction: ResMut<Interaction>,
) {
    let Some(controller) = controller else {
        return;
    };
    let Some(java) = controller.java.as_ref() else {
        *interaction = Default::default();
        return;
    };
    let state = &controller.java_world;
    let target = pick(state, &presentation, &movement, &view);
    let hit = target.as_ref().ok().copied().flatten();
    interaction.1.capture(state, &input.0, hit);
    let snapshot = input.0.snapshot();
    let released =
        snapshot.is_some_and(|s| s.authority_released) || interaction.1.blocked.is_some();
    interaction.0.begin_frame(
        state,
        time.delta(),
        snapshot.map(|s| s.authority_generation),
        released,
    );
    if let Err(error) = target {
        interaction.0.blocked = Some(error);
    }
    // A retained attack must enter the transport before a following slot change.
    for _ in 0..=interaction.1.backlog() {
        if !send_pending(java, &mut interaction.0) {
            return;
        }
        match interaction.1.front(state) {
            Some(Intent::Attack(pressed)) => {
                let (phase, captured_hit) = interaction.1.consume_attack(pressed);
                interaction.0.advance(state, phase, captured_hit);
            }
            Some(Intent::Use(pressed)) => {
                let (phase, captured_hit) = interaction.1.consume_use(pressed);
                interaction.0.use_on(state, phase, captured_hit);
            }
            Some(Intent::Slot(_) | Intent::Cycle(_)) => {
                let Some(selection) = interaction.1.selection(state) else {
                    continue;
                };
                match java.command(Command::Select {
                    dimension_epoch: selection.epoch,
                    sequence: selection.sequence,
                    slot: selection.slot,
                }) {
                    Ok(()) => interaction.1.selection_queued(),
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        interaction.0.blocked =
                            Some("Java selection is waiting for transport capacity".into());
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        interaction.0.blocked = Some("Java selection transport closed".into());
                    }
                }
                return;
            }
            None => {
                let use_phase = interaction.1.held_use();
                interaction.0.use_on(state, use_phase, hit);
                if !send_pending(java, &mut interaction.0) {
                    return;
                }
                let phase = interaction.1.held_attack();
                interaction.0.advance(state, phase, hit);
                send_pending(java, &mut interaction.0);
                return;
            }
        }
    }
}

/// Keeps the captured action unchanged until the bounded transport accepts it.
fn send_pending(
    java: &client_session::java::Session,
    interaction: &mut gameplay::java::interaction::Controller,
) -> bool {
    let Some(request) = interaction.pending() else {
        return true;
    };
    let face = match request.hit.face {
        selection::Face::Down => Face::Down,
        selection::Face::Up => Face::Up,
        selection::Face::North => Face::North,
        selection::Face::South => Face::South,
        selection::Face::West => Face::West,
        selection::Face::East => Face::East,
    };
    let command = match request.kind {
        Kind::UseOn => Command::UseOn {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            action: UseOn {
                hand: Hand::Main,
                position: request.hit.block,
                face,
                cursor: std::array::from_fn(|i| {
                    (request.hit.position[i] - f64::from(request.hit.block[i])) as f32
                }),
                inside: request.hit.inside,
                // The native outline picker returns a block hit, never a world-border hit.
                world_border: false,
                sequence: request.sequence,
            },
        },
        Kind::Break => Command::Interact {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            action: Dig {
                action: Action::Start,
                position: request.hit.block,
                face,
                sequence: request.sequence,
            },
        },
    };
    match java.command(command) {
        Ok(()) => {
            interaction.queued();
            return true;
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            interaction.blocked = Some("Java interaction is waiting for transport capacity".into())
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            interaction.blocked = Some("Java interaction transport closed".into())
        }
    }
    false
}

/// Captures click-time native outline authority; hotbar selection needs no geometry fallback.
fn pick(
    state: &client_world::java::WorldState,
    presentation: &Presentation,
    movement: &Movement,
    view: &LocalViewPose,
) -> Result<Option<selection::Hit>, String> {
    let (Some(shapes), Some(dimension), Some(motion)) = (
        presentation.interaction_shapes(),
        state.terrain.dimension(),
        state.player_motion,
    ) else {
        return Ok(None);
    };
    let range = state
        .player_dynamics
        .attributes
        .get("minecraft:block_interaction_range")
        .and_then(|a| a.value().ok())
        .ok_or("Missing native Java block interaction range")?;
    let mut from = motion.position;
    from[1] += movement.0.eye_height(state);
    let (yaw, pitch, _) = view.rotation().to_euler(EulerRot::YXZ);
    let direction = world::java::player::look(180. - yaw.to_degrees(), -pitch.to_degrees());
    let to = std::array::from_fn(|i| from[i] + direction[i] * range);
    selection::pick(
        &state.terrain,
        shapes,
        (dimension.session, dimension.epoch),
        from,
        to,
    )
    .map_err(|error| error.to_string())
}
