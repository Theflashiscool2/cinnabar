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
mod placement;
mod prediction;
mod swing;

#[derive(Resource, Default)]
pub(crate) struct Interaction(
    pub gameplay::java::interaction::Controller,
    pub Actions,
    Option<web_time::Instant>,
    Option<((u64, u64), u64)>,
    Option<((u64, u64), u64)>,
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
        state: &mut client_world::java::WorldState,
        movement: &Movement,
        shapes: Option<&::assets::registry::java::interaction::Table>,
        physics: &::assets::registry::java::physics::Table,
    ) {
        if !state.block_reconciliations.is_empty() {
            self.0.blocked = Some("Native block reconciliation requires collision facts".into());
            return;
        }
        let placement = Some(placement::Context {
            physics,
            body: movement.0.body,
        });
        let elapsed = self.elapsed();
        self.0.service_frame(state, elapsed);
        self.0.contact = Some((movement.0.body.on_ground, movement.0.body.eyes_in_water()));
        if !send_pending(java, state, &mut self.0, placement) {
            return;
        }
        if !self.finish_contact(java, state) {
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
        if send_pending(java, state, &mut self.0, placement) {
            self.continue_swing(java, state);
        }
    }

    pub(super) fn prepare_movement(
        &mut self,
        java: &client_session::java::Session,
        state: &mut client_world::java::WorldState,
        release: bool,
    ) -> bool {
        let was_using = self.0.using_item();
        self.0
            .maintain_use(state, state.inventory.cooldowns.client_tick(), release);
        if was_using
            && !self.0.using_item()
            && let Some(t) = java.telemetry()
        {
            use client_session::java::telemetry::Counter;
            let dead =
                state.death.is_some() || state.vitals.as_ref().is_none_or(|v| v.health <= 0.);
            t.count(
                if dead {
                    Counter::UseStoppedDead
                } else if release {
                    Counter::UseStoppedReleased
                } else {
                    Counter::UseStoppedOther
                },
                1,
            );
        }
        if !release && let Some(hand) = self.0.server_use_requested() {
            match item::resume(state, hand) {
                Ok(plan) => self.0.resume_server_use(state, plan),
                Err(error) => self.0.blocked = Some(error.into()),
            }
        }
        if self.0.pending().is_some_and(|r| r.kind == Kind::ReleaseUse) {
            send_pending(java, state, &mut self.0, None)
        } else {
            true
        }
    }

    /// Native continueAttack sends Punch while a valid held destroy target is active.
    /// The existing client timer bounds emission; rendering and the service loan share this receipt.
    fn continue_swing(
        &mut self,
        java: &client_session::java::Session,
        state: &mut client_world::java::WorldState,
    ) {
        if !self.1.held_attack().held
            || self.0.mining_progress().is_none()
            || self.0.blocked.is_some()
        {
            return;
        }
        let Some(d) = state.terrain.dimension() else {
            return;
        };
        let receipt = (
            (d.session, d.epoch),
            state.inventory.cooldowns.client_tick(),
        );
        if self.3 == Some(receipt) {
            return;
        }
        match java.command(Command::Punch {
            dimension_epoch: d.epoch,
            correction_sequence: state.motion_sequence(),
        }) {
            Ok(()) => {
                self.3 = Some(receipt);
                swing::predict(java, state);
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                self.0.blocked =
                    Some("Java mining gesture is waiting for transport capacity".into())
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                self.0.blocked = Some("Java mining gesture transport closed".into())
            }
        }
    }

    /// startAttack sends Punch after startDestroyBlock, even when destruction is denied.
    /// Retain that ordered tail through backpressure before accepting another item action.
    fn finish_contact(
        &mut self,
        java: &client_session::java::Session,
        state: &mut client_world::java::WorldState,
    ) -> bool {
        let Some((scope, correction)) = self.4 else {
            return true;
        };
        if state.terrain.dimension().map(|d| (d.session, d.epoch)) != Some(scope)
            || state.motion_sequence() != correction
            || state.death.is_some()
        {
            self.4 = None;
            return true;
        }
        match java.command(Command::Punch {
            dimension_epoch: scope.1,
            correction_sequence: correction,
        }) {
            Ok(()) => {
                self.4 = None;
                self.3 = Some((scope, state.inventory.cooldowns.client_tick()));
                swing::predict(java, state);
                true
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                self.0.blocked =
                    Some("Java contact gesture is waiting for transport capacity".into());
                false
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                self.0.blocked = Some("Java contact gesture transport closed".into());
                false
            }
        }
    }

    #[cfg(feature = "developer-control")]
    pub(crate) fn diagnostic(&self) -> serde_json::Value {
        let hit = |h: selection::Hit| serde_json::json!({"block":h.block,"state":h.state,"position":h.position,"face":format!("{:?}",h.face),"inside":h.inside});
        serde_json::json!({"selected":self.0.selected.map(hit),"last_queued":self.0.last_queued.map(|r|serde_json::json!({"kind":format!("{:?}",r.kind),"hand":format!("{:?}",r.hand),"sequence":r.sequence,"epoch":r.dimension_epoch,"hit":match r.target { RequestTarget::Block(h) => Some(hit(h)), RequestTarget::Air(_) | RequestTarget::Entity(_) => None },"target":match r.target { RequestTarget::Block(h) => serde_json::json!({"block":hit(h)}), RequestTarget::Air(rotation) => serde_json::json!({"air_rotation":rotation}), RequestTarget::Entity(e) => serde_json::json!({"entity":e.network_id,"lifetime":e.lifetime,"location":e.location,"secondary":e.secondary}) }})),"queued_actions":self.0.queued_actions,"mining":self.0.mining_progress().map(|(h,p)|serde_json::json!({"hit":hit(h),"progress":p})),"using_item":self.0.using_item(),"using_hand":self.0.using_hand().map(|hand|format!("{hand:?}")),"use_ticks":self.0.use_ticks(),"use_effects":self.0.use_effects().map(|e|serde_json::json!({"speed":e.speed_multiplier,"sprint":e.can_sprint})),"pending":self.0.pending().is_some(),"blocked":self.0.blocked,"backlog_ticks":self.0.backlog_ticks(),"prediction":self.0.predicted_blocks>0,"predicted_blocks":self.0.predicted_blocks,"scope":"native_mining_both_hand_block_entity_air_use_release_and_ordinary_melee", "selected_entity":self.1.selected.and_then(|t|match t { Target::Entity { network_id, identity, lifetime, location, secondary, .. } => Some(serde_json::json!({"network_id":network_id,"session":identity.session,"epoch":identity.epoch,"incarnation":identity.incarnation,"transport_lifetime":lifetime,"location":location,"secondary":secondary})), _=>None }),"entity_attacks_queued":self.1.entity_attacks_queued,"entity_uses_queued":self.0.entity_uses_queued,"input_backlog":self.1.backlog(), "input_blocked":self.1.blocked})
    }
}

/// Release is ordered before movement; a full channel retains both the action and
/// slowdown until it can be serialized. Menus/focus loss use the same semantic input.
pub(crate) fn retire_use(
    input: Res<SemanticInputSnapshot>,
    controller: Option<ResMut<SessionController>>,
    mut interaction: ResMut<Interaction>,
) {
    let Some(mut controller) = controller else {
        return;
    };
    let SessionController {
        java,
        java_world: state,
        ..
    } = &mut *controller;
    let Some(java) = java.as_ref() else {
        return;
    };
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
    let Some(mut controller) = controller else {
        return;
    };
    let SessionController {
        java,
        java_world: state,
        ..
    } = &mut *controller;
    let Some(java) = java.as_ref() else {
        *interaction = Default::default();
        return;
    };
    if !state.block_reconciliations.is_empty() {
        interaction.0.blocked = Some("Native block reconciliation requires collision facts".into());
        return;
    }
    let placement = presentation
        .movement_physics()
        .map(|physics| placement::Context {
            physics,
            body: movement.0.body,
        });
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
        Target::Block { hit, .. } => Some(hit),
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
        if !send_pending(java, state, &mut interaction.0, placement) {
            return;
        }
        if !interaction.finish_contact(java, state) {
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
                                Ok(()) => {
                                    interaction.1.entity_attack_queued();
                                    swing::predict(java, state);
                                }
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
                if pressed
                    && matches!(interaction.1.target(), Some(Target::Air { .. }))
                    && !released
                    && interaction.0.blocked.is_none()
                    && state.inventory.opened.is_none()
                    && presentation.world_ready() == Some(true)
                    && state.player_motion.is_some()
                    && state.status.mode != Some(world::java::hud::GameMode::Spectator)
                    && state.vitals.as_ref().is_some_and(|v| v.health > 0.)
                {
                    if let Err(error) = entity::ordinary_attack(state.inventory.held()) {
                        interaction.0.blocked = Some(error.into());
                    } else if let Some(dimension) = state.terrain.dimension() {
                        match java.command(Command::Punch {
                            dimension_epoch: dimension.epoch,
                            correction_sequence: state.motion_sequence(),
                        }) {
                            Ok(()) => {
                                swing::predict(java, state);
                            }
                            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                interaction.0.blocked =
                                    Some("Java attack is waiting for transport capacity".into());
                                return;
                            }
                            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                interaction.0.blocked = Some("Java attack transport closed".into());
                                return;
                            }
                        }
                    }
                }
                let contact = pressed
                    && matches!(interaction.1.target(), Some(Target::Block { .. }))
                    && !released
                    && state.inventory.opened.is_none()
                    && presentation.world_ready() == Some(true)
                    && !state.awaiting_correction_ack()
                    && state.status.mode != Some(world::java::hud::GameMode::Spectator)
                    && state.vitals.as_ref().is_some_and(|v| v.health > 0.);
                if contact && let Err(error) = entity::ordinary_attack(state.inventory.held()) {
                    interaction.0.blocked = Some(error.into());
                    interaction.1.consume_attack(pressed);
                    continue;
                }
                let (phase, captured_hit) = interaction.1.consume_attack(pressed);
                interaction.0.advance(state, phase, captured_hit);
                if contact && let Some(dimension) = state.terrain.dimension() {
                    interaction.4 = Some((
                        (dimension.session, dimension.epoch),
                        state.motion_sequence(),
                    ));
                }
            }
            Some(Intent::Use(pressed)) => {
                if !interaction.0.cancel_mining()
                    || !send_pending(java, state, &mut interaction.0, placement)
                {
                    return;
                }
                let captured = interaction.1.target();
                let (phase, _) = interaction.1.consume_use(pressed);
                advance_use(&mut interaction.0, state, phase, captured);
            }
            Some(Intent::Drop { whole_stack }) => {
                if state.status.mode != Some(world::java::hud::GameMode::Spectator) {
                    let Some(dimension) = state.terrain.dimension() else {
                        return;
                    };
                    let Some(sequence) = interaction.1.drop_sequence() else {
                        return;
                    };
                    let Some(slot) = state
                        .inventory
                        .selected
                        .and_then(|slot| u8::try_from(slot).ok())
                    else {
                        return;
                    };
                    if state.inventory.held().is_none() {
                        return;
                    }
                    match java.command(Command::Drop {
                        dimension_epoch: dimension.epoch,
                        correction_sequence: state.motion_sequence(),
                        sequence,
                        wire_revision: state.inventory.wire_revision,
                        slot,
                        whole_stack,
                    }) {
                        Ok(()) => {}
                        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                            interaction.0.blocked =
                                Some("Java drop is waiting for transport capacity".into());
                            return;
                        }
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                            interaction.0.blocked = Some("Java drop transport closed".into());
                            return;
                        }
                    }
                }
                interaction.1.drop_queued(whole_stack);
            }
            Some(Intent::Slot(_) | Intent::Cycle(_)) => {
                // Resolve first: selecting the current slot is a native no-op,
                // and must not release item use or abort ongoing mining.
                let Some(selection) = interaction.1.selection(state) else {
                    continue;
                };
                interaction
                    .0
                    .prepare_selection(state, state.inventory.cooldowns.client_tick());
                if !send_pending(java, state, &mut interaction.0, placement) {
                    return;
                }
                if !interaction.0.cancel_mining()
                    || !send_pending(java, state, &mut interaction.0, placement)
                {
                    return;
                }
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
                if !send_pending(java, state, &mut interaction.0, placement) {
                    return;
                }
                let phase = interaction.1.held_attack();
                interaction.0.advance(state, phase, hit);
                if send_pending(java, state, &mut interaction.0, placement) {
                    interaction.continue_swing(java, state);
                }
                return;
            }
        }
    }
}

/// Change-driven admission diagnostics; no server text, stack components or account data.
pub(crate) fn diagnose_mining(
    controller: Option<Res<SessionController>>,
    interaction: Res<Interaction>,
    input: Res<SemanticInputSnapshot>,
    mut previous: Local<Option<(u64, Option<String>)>>,
) {
    let Some(world) = controller.as_ref().and_then(|s| s.native_java_world()) else {
        *previous = None;
        return;
    };
    let phase = input.0.phase(semantic_input::Action::Attack);
    if !phase.held && !phase.pressed {
        *previous = None;
        return;
    }
    let reason = interaction.0.blocked.clone().or_else(|| {
        interaction
            .0
            .using_item()
            .then(|| "An active item use owns attack input".into())
    });
    let current = (world.generation(), reason.clone());
    if previous.as_ref() != Some(&current) || phase.pressed {
        bevy::log::debug!(target:"java_session", reason=?reason, mode=?world.status.mode,
            has_block_target=interaction.0.selected.is_some(), using_item=interaction.0.using_item(),
            queued_actions=interaction.0.queued_actions, correction_pending=world.awaiting_correction_ack(),
            "Native Java mining input state");
        *previous = Some(current);
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
    if !interaction.use_due(phase) {
        return;
    }
    let steps = item::steps(state, target);
    if phase.pressed
        && steps.is_ok()
        && state
            .inventory
            .slot(0, inventory::java::OFFHAND_MENU_SLOT)
            .is_none()
    {
        bevy::log::debug!(target:"java_session","Native main-hand use retained; offhand awaits authoritative slot data");
    }
    if phase.pressed
        && let Err(error) = steps
    {
        bevy::log::warn!(target:"java_session",error,"Native item use rejected before transport");
    }
    interaction.use_chain(state, phase, steps);
}

/// Keeps the captured action unchanged until the bounded transport accepts it.
fn send_pending(
    java: &client_session::java::Session,
    state: &mut client_world::java::WorldState,
    interaction: &mut gameplay::java::interaction::Controller,
    placement: Option<placement::Context<'_>>,
) -> bool {
    if !state.block_reconciliations.is_empty()
        && interaction
            .pending()
            .is_some_and(|request| !matches!(request.kind, Kind::ReleaseUse | Kind::AbortBreak))
    {
        interaction.blocked = Some("Native block reconciliation requires collision facts".into());
        return false;
    }
    if !interaction.validate_use_entity(state) {
        return true;
    }
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
            return send_pending(java, state, interaction, placement);
        }
    }
    let command = match (request.kind, request.target) {
        (Kind::UseEntity, RequestTarget::Entity(target)) => Command::UseEntity {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            lifetime: target.lifetime,
            action: java_protocol::session::interaction::EntityUse {
                entity: target.network_id,
                hand: protocol_hand(request.hand),
                location: target.location,
                secondary: target.secondary,
            },
        },
        (Kind::ReleaseUse, _) => Command::Interact {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            action: Dig::release_use(),
        },
        (Kind::UseAir, RequestTarget::Air(rotation)) => Command::UseItem {
            dimension_epoch: request.dimension_epoch,
            correction_sequence: request.correction_sequence,
            action: java_protocol::session::interaction::UseItem {
                hand: protocol_hand(request.hand),
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
                        hand: protocol_hand(request.hand),
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
    let placement_preview = (request.kind == Kind::UseOn)
        .then(|| placement.and_then(|context| placement::preview(state, &request, context)))
        .flatten();
    match java.command(command) {
        Ok(()) => {
            state.terrain.prediction_sequence(request.sequence);
            if prediction::apply(
                state,
                &request,
                interaction.pending_destroys_block(),
                placement,
            ) {
                interaction.predicted_blocks = interaction.predicted_blocks.saturating_add(1);
                if let Some(telemetry) = java.telemetry() {
                    telemetry.prediction(client_session::java::telemetry::Prediction::Applied, 1);
                }
            }
            if let Some(collision) = placement_preview.filter(|p| p.gesture()) {
                bevy::log::debug!(target: "java_session", sequence=request.sequence, hand=?request.hand,
                    ?collision, "Native placement attempt gesture");
                swing::predict_interact(java, state, request.hand);
            }
            interaction.queued();
            // One captured click has at most four ordered main/offhand target/air stages.
            return send_pending(java, state, interaction, placement);
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

fn protocol_hand(hand: gameplay::java::item_use::plan::Hand) -> Hand {
    match hand {
        gameplay::java::item_use::plan::Hand::Main => Hand::Main,
        gameplay::java::item_use::plan::Hand::Off => Hand::Off,
    }
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
