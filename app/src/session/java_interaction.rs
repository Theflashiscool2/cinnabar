//! Binds edition-owned block selection and intent to native transport; no Bedrock packets.
use super::{SessionController, java_movement::Movement, java_render::Presentation};
use bevy::{prelude::*, time::Real};
use client_presentation::local_player::LocalViewPose;
use client_state::SemanticInputSnapshot;
use java_protocol::session::{
    Command,
    interaction::{Action, Dig, Face},
};
use semantic_input::Action as InputAction;
use sim::java::selection;

#[derive(Resource, Default)]
pub(crate) struct Interaction(pub gameplay::java::interaction::Controller);
impl Interaction {
    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        let hit = |h: selection::Hit| serde_json::json!({"block":h.block,"state":h.state,"position":h.position,"face":format!("{:?}",h.face),"inside":h.inside});
        serde_json::json!({"selected":self.0.selected.map(hit),"last_queued":self.0.last_queued.map(|r|serde_json::json!({"sequence":r.sequence,"epoch":r.dimension_epoch,"hit":hit(r.hit)})),"queued_actions":self.0.queued_actions,"pending":self.0.pending().is_some(),"blocked":self.0.blocked,"backlog_ticks":self.0.backlog_ticks(),"prediction":false,"scope":"creative_breaking"})
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
        interaction.0 = Default::default();
        return;
    };
    let state = &controller.java_world;
    let snapshot = input.0.snapshot();
    interaction.0.begin_frame(
        state,
        time.delta(),
        snapshot.map(|s| s.authority_generation),
        snapshot.is_some_and(|s| s.authority_released),
    );
    let Some(shapes) = presentation.interaction_shapes() else {
        return;
    };
    if interaction.0.pending().is_none() {
        let Some(dimension) = state.terrain.dimension() else {
            return;
        };
        let Some(motion) = state.player_motion else {
            return;
        };
        let Some(range) = state
            .player_dynamics
            .attributes
            .get("minecraft:block_interaction_range")
            .and_then(|a| a.value().ok())
        else {
            interaction.0.blocked = Some("Missing native Java block interaction range".into());
            return;
        };
        let mut from = motion.position;
        from[1] += movement.0.eye_height(state);
        let (yaw, pitch, _) = view.rotation().to_euler(EulerRot::YXZ);
        let direction = world::java::player::look(180. - yaw.to_degrees(), -pitch.to_degrees());
        let to = std::array::from_fn(|i| from[i] + direction[i] * range);
        let hit = match selection::pick(
            &state.terrain,
            shapes,
            (dimension.session, dimension.epoch),
            from,
            to,
        ) {
            Ok(hit) => hit,
            Err(error) => {
                interaction.0.selected = None;
                interaction.0.blocked = Some(error.to_string());
                return;
            }
        };
        interaction
            .0
            .advance(state, input.0.phase(InputAction::Attack), hit);
    }
    let Some(request) = interaction.0.pending() else {
        return;
    };
    let face = match request.hit.face {
        selection::Face::Down => Face::Down,
        selection::Face::Up => Face::Up,
        selection::Face::North => Face::North,
        selection::Face::South => Face::South,
        selection::Face::West => Face::West,
        selection::Face::East => Face::East,
    };
    let command = Command::Interact {
        dimension_epoch: request.dimension_epoch,
        correction_sequence: request.correction_sequence,
        action: Dig {
            action: Action::Start,
            position: request.hit.block,
            face,
            sequence: request.sequence,
        },
    };
    match java.command(command) {
        Ok(()) => interaction.0.queued(),
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            interaction.0.blocked =
                Some("Java interaction is waiting for transport capacity".into())
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            interaction.0.blocked = Some("Java interaction transport closed".into())
        }
    }
}
