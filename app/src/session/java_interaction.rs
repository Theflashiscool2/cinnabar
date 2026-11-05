//! Binds edition-owned block selection and intent to native transport; no Bedrock packets.
use super::{SessionController, java_movement::Movement, java_render::Presentation};
use bevy::prelude::*;
use client_presentation::local_player::LocalViewPose;
use client_state::SemanticInputSnapshot;
use gameplay::java::actions::{Controller as Actions, Intent, Target};
use gameplay::java::interaction::{Kind, Target as RequestTarget};
use java_protocol::session::{
    Command,
    interaction::{Action, Dig, Face, Hand, UseOn},
};
use sim::java::selection;
mod entity;
mod item;

#[derive(Resource, Default)]
pub(crate) struct Interaction(
    pub gameplay::java::interaction::Controller,
    pub Actions,
    Option<web_time::Instant>,
);
impl Interaction {
    pub(super) fn elapsed(&mut self) -> std::time::Duration {
        let now = web_time::Instant::now();
        self.2
            .replace(now)
            .map_or(std::time::Duration::ZERO, |last| {
                now.saturating_duration_since(last)
            })
    }
    /// Continue only an existing held target. Re-pick against the current simulated pose
    /// and current authoritative blocks; a stale click never mines through a new obstacle.
    pub(super) fn service(
        &mut self,
        java: &client_session::java::Session,
        state: &client_world::java::WorldState,
        movement: &Movement,
        shapes: Option<&::assets::registry::java::interaction::Table>,
    ) {
        let elapsed = self.elapsed();
        self.0.service_frame(state, elapsed);
        self.0.contact = Some((movement.0.body.on_ground, movement.0.body.eyes_in_water()));
        if !send_pending(java, state, &mut self.0) {
            return;
        }
        if self.0.mining_progress().is_none() {
            return;
        }
        let hit = (|| {
            let shapes = shapes?;
            let dimension = state.terrain.dimension()?;
            let motion = state.player_motion?;
            let range = state
                .player_dynamics
                .attributes
                .get("minecraft:block_interaction_range")?
                .value()
                .ok()?;
            let mut from = motion.position;
            from[1] += movement.0.eye_height(state);
            let direction = world::java::player::look(motion.yaw, motion.pitch);
            let to = std::array::from_fn(|i| from[i] + direction[i] * range);
            selection::pick(
                &state.terrain,
                shapes,
                (dimension.session, dimension.epoch),
                from,
                to,
            )
            .ok()
            .flatten()
        })();
        self.0.advance(state, self.1.held_attack(), hit);
        send_pending(java, state, &mut self.0);
    }

    pub(super) fn prepare_movement(
        &mut self,
        java: &client_session::java::Session,
        state: &client_world::java::WorldState,
        release: bool,
    ) -> bool {
        self.0
            .maintain_use(state, state.inventory.cooldowns.client_tick(), release);
        if !release && self.0.server_use_requested() == Some(false) {
            match item::resume(state) {
                Ok(plan) => self.0.resume_server_use(state, plan),
                Err(error) => self.0.blocked = Some(error.into()),
            }
        }
        if self.0.pending().is_some_and(|r| r.kind == Kind::ReleaseUse) {
            send_pending(java, state, &mut self.0)
        } else {
            true
        }
    }

    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        let hit = |h: selection::Hit| serde_json::json!({"block":h.block,"state":h.state,"position":h.position,"face":format!("{:?}",h.face),"inside":h.inside});
        serde_json::json!({"selected":self.0.selected.map(hit),"last_queued":self.0.last_queued.map(|r|serde_json::json!({"kind":format!("{:?}",r.kind),"sequence":r.sequence,"epoch":r.dimension_epoch,"hit":match r.target { RequestTarget::Block(h) => Some(hit(h)), RequestTarget::Air(_) => None },"target":match r.target { RequestTarget::Block(h) => serde_json::json!({"block":hit(h)}), RequestTarget::Air(rotation) => serde_json::json!({"air_rotation":rotation}) }})),"queued_actions":self.0.queued_actions,"mining":self.0.mining_progress().map(|(h,p)|serde_json::json!({"hit":hit(h),"progress":p})),"using_item":self.0.using_item(),"use_ticks":self.0.use_ticks(),"use_effects":self.0.use_effects().map(|e|serde_json::json!({"speed":e.speed_multiplier,"sprint":e.can_sprint})),"pending":self.0.pending().is_some(),"blocked":self.0.blocked,"backlog_ticks":self.0.backlog_ticks(),"prediction":false,"scope":"native_mining_main_hand_use_release_and_ordinary_melee", "selected_entity":self.1.selected.and_then(|t|match t { Target::Entity { network_id, identity, lifetime, location, secondary, .. } => Some(serde_json::json!({"network_id":network_id,"session":identity.session,"epoch":identity.epoch,"incarnation":identity.incarnation,"transport_lifetime":lifetime,"location":location,"secondary":secondary})), _=>None }),"entity_attacks_queued":self.1.entity_attacks_queued,"entity_uses_queued":self.1.entity_uses_queued,"input_backlog":self.1.backlog(), "input_blocked":self.1.blocked})
    }
}

/// Release is ordered before movement; a full channel retains both the action and
/// slowdown until it can be serialized. Menus/focus loss use the same semantic input.
pub(crate) fn retire_use(
    input: Res<SemanticInputSnapshot>,
    controller: Option<ResMut<SessionController>>,
    mut interaction: ResMut<Interaction>,
) {
    let Some(controller) = controller else { return };
    let Some(java) = controller.java.as_ref() else {
        return;
    };
    let state = &controller.java_world;
    let release = !input.0.phase(semantic_input::Action::Use).held
        || input.0.snapshot().is_none_or(|s| s.authority_released)
        || state.inventory.opened.is_some();
    interaction.prepare_movement(java, state, release);
}

#[allow(
    clippy::too_many_arguments,
    reason = "host binds session, input, camera and native asset owners"
)]
pub(crate) fn update(
    input: Res<SemanticInputSnapshot>,
    view: Res<LocalViewPose>,
    controller: Option<ResMut<SessionController>>,
    presentation: Res<Presentation>,
    special: Res<super::java_special::Presentation>,
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
    let target = pick(
        state,
        &presentation,
        &special,
        &movement,
        &view,
        input.0.phase(semantic_input::Action::Sneak).held,
    );
    let chosen = target.as_ref().ok().copied().flatten();
    let hit = chosen.and_then(|t| match t {
        Target::Block(h) => Some(h),
        _ => None,
    });
    interaction.1.capture_target(state, &input.0, chosen);
    let snapshot = input.0.snapshot();
    let released =
        snapshot.is_some_and(|s| s.authority_released) || interaction.1.blocked.is_some();
    let elapsed = interaction.elapsed();
    interaction.0.begin_frame(
        state,
        elapsed,
        snapshot.map(|s| s.authority_generation),
        released,
    );
    interaction.0.contact = Some((movement.0.body.on_ground, movement.0.body.eyes_in_water()));
    if let Err(error) = target {
        interaction.0.blocked = Some(error);
    }
    // A retained attack must enter the transport before a following slot change.
    for _ in 0..=interaction.1.backlog() {
        if !send_pending(java, state, &mut interaction.0) {
            return;
        }
        match interaction.1.front(state) {
            Some(Intent::Attack(pressed)) => {
                if interaction.0.using_item() {
                    interaction.1.consume_attack(pressed);
                    continue;
                }
                if pressed
                    && let Some(Target::Entity {
                        network_id,
                        identity,
                        lifetime,
                        ..
                    }) = interaction.1.target()
                {
                    // Keep the physical edge at the head of the SAME ordered queue until accepted.
                    if state
                        .entities
                        .get(network_id)
                        .is_some_and(|e| e.identity == identity && e.transport_lifetime == lifetime)
                    {
                        if let Err(error) = entity::ordinary_attack(state.inventory.held()) {
                            interaction.0.blocked = Some(error.into());
                            bevy::log::warn!(target: "java_session", error, "Native entity attack rejected before transport");
                        } else {
                            match java.command(Command::Attack {
                                dimension_epoch: identity.epoch,
                                correction_sequence: state.motion_sequence(),
                                entity: network_id,
                                lifetime,
                            }) {
                                Ok(()) => interaction.1.entity_attack_queued(),
                                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                    interaction.0.blocked = Some(
                                        "Java attack is waiting for transport capacity".into(),
                                    );
                                    return;
                                }
                                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                    interaction.0.blocked =
                                        Some("Java attack transport closed".into());
                                    return;
                                }
                            }
                        }
                    }
                    let (phase, _) = interaction.1.consume_attack(pressed);
                    interaction.0.advance(state, phase, None);
                    continue;
                }
                let (phase, captured_hit) = interaction.1.consume_attack(pressed);
                interaction.0.advance(state, phase, captured_hit);
            }
            Some(Intent::Use(pressed)) => {
                if !interaction.0.cancel_mining() || !send_pending(java, state, &mut interaction.0)
                {
                    return;
                }
                let captured = interaction.1.target();
                if pressed
                    && let Some(Target::Entity {
                        network_id,
                        identity,
                        lifetime,
                        location,
                        rotation,
                        secondary,
                    }) = captured
                {
                    if state
                        .entities
                        .get(network_id)
                        .is_some_and(|e| e.identity == identity && e.transport_lifetime == lifetime)
                    {
                        match java.command(Command::UseEntity {
                            dimension_epoch: identity.epoch,
                            correction_sequence: state.motion_sequence(),
                            lifetime,
                            action: java_protocol::session::interaction::EntityUse {
                                entity: network_id,
                                hand: Hand::Main,
                                location,
                                secondary,
                            },
                        }) {
                            Ok(()) => interaction.1.entity_use_queued(),
                            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                interaction.0.blocked = Some(
                                    "Java entity use is waiting for transport capacity".into(),
                                );
                                return;
                            }
                            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                interaction.0.blocked =
                                    Some("Java entity use transport closed".into());
                                return;
                            }
                        }
                        let (phase, _) = interaction.1.consume_use(pressed);
                        // Ordinary instant item fallback stays on the same click-time ray. Active
                        // use/offhand prediction remains unsupported; never consume locally.
                        if entity::allows_instant_fallback(state, network_id)
                            && item::instant(state) == Ok(true)
                        {
                            advance_use(
                                &mut interaction.0,
                                state,
                                phase,
                                Some(Target::Air { rotation }),
                            );
                        } else {
                            interaction.0.use_target(state, phase, Ok(None));
                        }
                        continue;
                    }
                    interaction.1.consume_use(pressed);
                    continue;
                }
                let (phase, _) = interaction.1.consume_use(pressed);
                advance_use(&mut interaction.0, state, phase, captured);
            }
            Some(Intent::Slot(_) | Intent::Cycle(_)) => {
                interaction
                    .0
                    .maintain_use(state, state.inventory.cooldowns.client_tick(), true);
                if !send_pending(java, state, &mut interaction.0) {
                    return;
                }
                if !interaction.0.cancel_mining() || !send_pending(java, state, &mut interaction.0)
                {
                    return;
                }
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
                advance_use(&mut interaction.0, state, use_phase, chosen);
                if !send_pending(java, state, &mut interaction.0) {
                    return;
                }
                let phase = interaction.1.held_attack();
                interaction.0.advance(state, phase, hit);
                send_pending(java, state, &mut interaction.0);
                return;
            }
        }
    }
}

fn advance_use(
    interaction: &mut gameplay::java::interaction::Controller,
    state: &client_world::java::WorldState,
    phase: semantic_input::ActionPhase,
    target: Option<Target>,
) {
    if interaction.using_item() {
        return;
    }
    if !phase.pressed && !phase.held {
        interaction.use_target(state, phase, Ok(None));
        return;
    }
    let plan = item::plan(state);
    let fallback = if let Some(Target::Block(hit)) = target {
        let rotation = state.player_motion.map(|m| [m.yaw, m.pitch]);
        let allowed = item::block_passes(state, hit) == Ok(true);
        rotation
            .filter(|_| allowed)
            .zip(plan.as_ref().ok().copied())
    } else {
        None
    };
    let target = match target {
        Some(Target::Block(hit)) => Ok(Some(RequestTarget::Block(hit))),
        Some(Target::Air { rotation }) => plan
            .as_ref()
            .map(|p| p.send.then_some(RequestTarget::Air(rotation)))
            .map_err(|e| *e),
        // Presses were admitted by the entity branch above; a release has no entity packet.
        Some(Target::Entity { .. }) => Ok(None),
        None => Err("Native item use requires a loaded targeting snapshot"),
    };
    if phase.pressed
        && let Err(error) = target
    {
        bevy::log::warn!(target: "java_session", error, "Native item use rejected before transport");
    }
    interaction.use_target(state, phase, target);
    if let Ok(plan) = plan {
        interaction.prepare_use(state, plan);
    }
    if let Some((rotation, plan)) = fallback {
        interaction.prepare_use_fallback(state, rotation, plan);
    }
}

/// Keeps the captured action unchanged until the bounded transport accepts it.
pub(super) fn send_pending(
    java: &client_session::java::Session,
    state: &client_world::java::WorldState,
    interaction: &mut gameplay::java::interaction::Controller,
) -> bool {
    let Some(request) = interaction.pending() else {
        return true;
    };
    if matches!(request.kind, Kind::Break | Kind::FinishBreak) {
        let current = match request.target {
            RequestTarget::Block(hit) => state.terrain.block(hit.block).map(|id| id.0),
            _ => None,
        };
        if !interaction.validate_mining_target(current) {
            // Invalidating an already started action leaves only an ABORT, so recursion
            // is bounded to one cancellation and never sends an obsolete finish.
            return send_pending(java, state, interaction);
        }
    }
    let command = match (request.kind, request.target) {
        (Kind::ReleaseUse, _) => Command::Interact {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            action: Dig::release_use(),
        },
        (Kind::UseAir, RequestTarget::Air(rotation)) => Command::UseItem {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            action: java_protocol::session::interaction::UseItem {
                hand: Hand::Main,
                sequence: request.sequence,
                rotation,
            },
        },
        (
            kind @ (Kind::Break | Kind::AbortBreak | Kind::FinishBreak | Kind::UseOn),
            RequestTarget::Block(hit),
        ) => {
            let face = match hit.face {
                selection::Face::Down => Face::Down,
                selection::Face::Up => Face::Up,
                selection::Face::North => Face::North,
                selection::Face::South => Face::South,
                selection::Face::West => Face::West,
                selection::Face::East => Face::East,
            };
            if kind == Kind::UseOn {
                Command::UseOn {
                    dimension_epoch: request.dimension_epoch,
                    correction_sequence: request.correction_sequence,
                    action: UseOn {
                        hand: Hand::Main,
                        position: hit.block,
                        face,
                        cursor: std::array::from_fn(|i| {
                            (hit.position[i] - f64::from(hit.block[i])) as f32
                        }),
                        inside: hit.inside,
                        world_border: false,
                        sequence: request.sequence,
                    },
                }
            } else {
                Command::Interact {
                    dimension_epoch: request.dimension_epoch,
                    correction_sequence: request.correction_sequence,
                    action: Dig {
                        action: match kind {
                            Kind::AbortBreak => Action::Abort,
                            Kind::FinishBreak => Action::Finish,
                            _ => Action::Start,
                        },
                        position: hit.block,
                        face: if kind == Kind::AbortBreak {
                            Face::Down
                        } else {
                            face
                        },
                        sequence: request.sequence,
                    },
                }
            }
        }
        _ => {
            interaction.blocked = Some("Invalid native interaction target/kind".into());
            return false;
        }
    };
    match java.command(command) {
        Ok(()) => {
            interaction.queued();
            // The only follow-up is one guaranteed block PASS -> air request.
            return send_pending(java, state, interaction);
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
    special: &super::java_special::Presentation,
    movement: &Movement,
    view: &LocalViewPose,
    secondary: bool,
) -> Result<Option<Target>, String> {
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
    let [yaw, pitch] =
        super::java_look::native_angles(view.rotation(), Some([motion.yaw, motion.pitch]));
    let direction = world::java::player::look(yaw, pitch);
    let entity_range = state
        .player_dynamics
        .attributes
        .get("minecraft:entity_interaction_range")
        .and_then(|a| a.value().ok())
        .ok_or("Missing native Java entity interaction range")?;
    let to = std::array::from_fn(|i| from[i] + direction[i] * range.max(entity_range));
    let block = selection::pick_with_context(
        &state.terrain,
        shapes,
        (dimension.session, dimension.epoch),
        from,
        to,
        |position, id| special.outline(&state.terrain, position, id),
    )
    .map_err(|error| error.to_string())?;
    Ok(entity::pick(
        state,
        from,
        to,
        entity_range,
        range,
        block,
        ([yaw, pitch], secondary),
    )
    .or(Some(Target::Air {
        rotation: [yaw, pitch],
    })))
}
