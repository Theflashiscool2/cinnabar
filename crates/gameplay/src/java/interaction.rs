//! Native block intent. Shared action sequences never predict terrain or stack changes.
use super::item_use::plan::Hand;
use client_world::java::WorldState;
use semantic_input::{ActionPhase, identity::InputAuthorityGeneration};
use sim::java::selection::Hit;
use std::{collections::VecDeque, time::Duration};
mod chain;
pub use chain::UseStep;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Break,
    AbortBreak,
    FinishBreak,
    UseOn,
    UseAir,
    UseEntity,
    ReleaseUse,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Block(Hit),
    Air([f32; 2]),
    Entity(EntityTarget),
}
/// Captured native entity identity and hit; a reused network ID is never equivalent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityTarget {
    pub network_id: i32,
    pub identity: world::java::entities::Identity,
    pub lifetime: u64,
    pub location: [f64; 3],
    pub secondary: bool,
}
impl EntityTarget {
    pub fn current(self, state: &WorldState) -> bool {
        state
            .entities
            .get(self.network_id)
            .is_some_and(|e| e.identity == self.identity && e.transport_lifetime == self.lifetime)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub hand: Hand,
    pub kind: Kind,
    pub dimension_epoch: u64,
    pub correction_sequence: u64,
    pub sequence: i32,
    pub target: Target,
}
struct Mining {
    hit: Hit,
    item: inventory::java::Slot,
    progress: f32,
    started: bool,
}
#[derive(Default)]
pub struct Controller {
    scope: Option<(u64, u64)>,
    authority: Option<InputAuthorityGeneration>,
    enabled: bool,
    correction: u64,
    sequence: i32,
    elapsed: Duration,
    use_elapsed: Duration,
    use_remainder: Duration,
    delay: u8,
    pending: Option<Request>,
    mining: Option<Mining>,
    creative: bool,
    active_use: Option<super::item_use::active::Using>,
    prepared_use: Option<(Hand, inventory::java::Stack, super::item_use::plan::Active)>,
    use_tick: u64,
    use_revision: u64,
    server_use: Option<Hand>,
    use_steps: VecDeque<UseStep>,
    use_entity: Option<EntityTarget>,
    /// Supplied by the sole native collision simulation, not an optimistic on-ground flag.
    pub contact: Option<(bool, bool)>,
    pub selected: Option<Hit>,
    pub last_queued: Option<Request>,
    pub queued_actions: u64,
    pub entity_uses_queued: u64,
    pub blocked: Option<String>,
}
impl Controller {
    /// Retires unsent intent on input, dimension or correction boundaries; receipts do not
    /// predict success and never edit the terrain. The socket owner checks the same fences.
    pub fn begin_frame(
        &mut self,
        state: &WorldState,
        elapsed: Duration,
        authority: Option<InputAuthorityGeneration>,
        released: bool,
    ) {
        let scope = state.terrain.dimension().map(|d| (d.session, d.epoch));
        if self.scope != scope {
            *self = Self {
                scope,
                ..Default::default()
            };
        }
        let correction = state.motion_sequence();
        if self.authority != authority
            || released
            || self.correction != correction
            || state.awaiting_correction_ack()
        {
            if self
                .pending
                .is_none_or(|r| !matches!(r.kind, Kind::AbortBreak | Kind::ReleaseUse))
                || self.correction != correction
                || state.awaiting_correction_ack()
            {
                self.pending = None;
            }
            if self.correction == correction && !state.awaiting_correction_ack() {
                self.cancel_mining();
            } else {
                self.mining = None;
            }
            self.prepared_use = None;
            self.use_steps.clear();
            self.use_entity = None;
            self.elapsed = Duration::ZERO;
            self.use_elapsed = Duration::ZERO;
            self.delay = 0;
            self.selected = None;
        } else {
            self.elapsed = self.elapsed.saturating_add(elapsed);
            self.use_elapsed = self.use_elapsed.saturating_add(elapsed);
        }
        self.authority = authority;
        self.enabled = authority.is_some() && !released;
        self.correction = correction;
        let invalid_break = state.inventory.held().is_none()
            || !matches!(
                state.status.mode,
                Some(world::java::hud::GameMode::Survival | world::java::hud::GameMode::Creative)
            );
        if self.pending.is_some_and(|request| match request.kind {
            Kind::Break | Kind::FinishBreak => invalid_break,
            Kind::AbortBreak | Kind::ReleaseUse => false,
            Kind::UseOn | Kind::UseAir | Kind::UseEntity => !can_use(state),
        }) || state.death.is_some()
            || state.vitals.as_ref().is_none_or(|v| v.health <= 0.)
        {
            self.pending = None;
            self.use_steps.clear();
            self.use_entity = None;
            self.prepared_use = None;
            self.mining = None;
            self.elapsed = Duration::ZERO;
            self.use_elapsed = Duration::ZERO;
        }
        if self.using_item() {
            // Native using-item frames do not advance block breaking. The right-click
            // delay can expire, but cannot accumulate a burst of repeated uses.
            self.elapsed = Duration::ZERO;
            self.use_elapsed = self.use_elapsed.min(simulation_clock::TICK_DURATION * 4);
        }
        self.blocked = None;
        self.use_tick = state.inventory.cooldowns.client_tick();
    }
    /// The renderer may be stalled; the same owner continues an already captured hold.
    pub fn service_frame(&mut self, state: &WorldState, elapsed: Duration) {
        self.begin_frame(state, elapsed, self.authority, !self.enabled);
    }
    pub fn pending(&self) -> Option<Request> {
        self.pending
    }
    pub fn queued(&mut self) {
        if let Some(request) = self.pending.take() {
            self.last_queued = Some(request);
            self.queued_actions = self.queued_actions.saturating_add(1);
            // Backpressure paused this action; old elapsed time must not become fresh presses.
            match request.kind {
                Kind::Break => {
                    if let Some(mining) = self.mining.as_mut() {
                        mining.started = true;
                    }
                    if self.creative {
                        self.delay = 5;
                    }
                    self.elapsed = Duration::ZERO;
                }
                Kind::FinishBreak => {
                    self.mining = None;
                    self.delay = 5;
                    self.elapsed = Duration::ZERO;
                }
                Kind::AbortBreak => {
                    self.mining = None;
                    self.elapsed = Duration::ZERO;
                }
                Kind::UseOn | Kind::UseAir | Kind::UseEntity => {
                    if request.kind == Kind::UseEntity {
                        self.entity_uses_queued = self.entity_uses_queued.saturating_add(1);
                    }
                    self.use_elapsed = self.use_remainder;
                    if request.kind == Kind::UseAir {
                        self.active_use = self.prepared_use.take().map(|(hand, item, plan)| {
                            super::item_use::active::Using::start_in(
                                hand,
                                item,
                                plan,
                                self.use_tick,
                            )
                        });
                    }
                }
                Kind::ReleaseUse => {
                    self.server_use = None;
                    self.active_use = None;
                    self.prepared_use = None;
                }
            }
            self.advance_use_step(request.kind);
        }
    }
    /// Called before movement and by the between-frame owner. A physical release is
    /// retained ahead of later movement/slot packets when transport is backpressured.
    pub fn maintain_use(&mut self, state: &WorldState, tick: u64, release: bool) {
        self.use_tick = tick;
        let scope = state.terrain.dimension().map(|d| (d.session, d.epoch));
        if scope != self.scope
            || state.death.is_some()
            || state.vitals.as_ref().is_none_or(|v| v.health <= 0.)
        {
            self.active_use = None;
            self.prepared_use = None;
            self.use_steps.clear();
            self.use_entity = None;
            self.server_use = None;
            return;
        }
        if self.use_revision != state.player_visual.use_revision {
            self.use_revision = state.player_visual.use_revision;
            if state.player_visual.use_flags & 1 == 0 {
                self.active_use = None;
                self.server_use = None;
            } else {
                let hand = if state.player_visual.use_flags & 2 != 0 {
                    Hand::Off
                } else {
                    Hand::Main
                };
                if self
                    .active_use
                    .as_ref()
                    .is_none_or(|using| using.hand() != hand)
                {
                    self.active_use = None;
                    self.server_use = Some(hand);
                }
            }
        }
        if self
            .active_use
            .as_ref()
            .is_some_and(|u| !u.matches(u.hand().held(&state.inventory)))
        {
            self.active_use = None;
        }
        if release && (self.active_use.is_some() || self.server_use.is_some()) {
            if self.pending.is_none() && self.request_target(Target::Air([0.; 2]), Kind::ReleaseUse)
            {
                // Retain slowdown until the release has actually entered transport.
                self.prepared_use = None;
            }
        }
    }
    pub fn prepare_use_fallback(
        &mut self,
        state: &WorldState,
        rotation: [f32; 2],
        plan: super::item_use::plan::Plan,
    ) {
        if self.pending.is_some_and(|r| r.kind == Kind::UseOn) && plan.send {
            self.use_revision = state.player_visual.use_revision;
            self.use_steps.clear();
            self.use_entity = None;
            if let Some(slot) = state.inventory.held() {
                match UseStep::air(rotation, Hand::Main, plan, slot) {
                    Ok(step) => self.use_steps.push_back(step),
                    Err(error) => self.blocked = Some(error.into()),
                }
            }
        }
    }
    pub fn prepare_use(&mut self, state: &WorldState, plan: super::item_use::plan::Plan) {
        if self.pending.is_some_and(|r| r.kind == Kind::UseAir) {
            self.use_revision = state.player_visual.use_revision;
            self.prepared_use = match plan.result {
                super::item_use::plan::ResultKind::Consume(Some(active)) => state
                    .inventory
                    .held()
                    .and_then(Option::as_ref)
                    .map(|s| (Hand::Main, s.clone(), active)),
                _ => None,
            };
        }
    }
    pub fn server_use_requested(&self) -> Option<Hand> {
        self.server_use
    }
    pub fn resume_server_use(&mut self, state: &WorldState, plan: super::item_use::plan::Plan) {
        if let Some(hand) = self.server_use {
            if let (Some(item), super::item_use::plan::ResultKind::Consume(Some(active))) = (
                hand.held(&state.inventory).and_then(Option::as_ref),
                plan.result,
            ) {
                self.active_use = Some(super::item_use::active::Using::start_in(
                    hand,
                    item.clone(),
                    active,
                    self.use_tick,
                ));
                self.server_use = None;
            }
        }
    }
    pub fn using_hand(&self) -> Option<Hand> {
        self.active_use
            .as_ref()
            .map(|u| u.hand())
            .or(self.server_use)
    }
    pub fn using_item(&self) -> bool {
        self.active_use.is_some() || self.server_use.is_some()
    }
    pub fn use_effects(&self) -> Option<super::item_use::plan::Effects> {
        self.active_use.as_ref().map(|u| u.effects())
    }
    pub fn use_duration(&self) -> Option<u32> {
        self.active_use.as_ref().map(|u| u.duration())
    }
    pub fn use_ticks(&self) -> Option<u32> {
        self.active_use.as_ref().map(|u| u.elapsed(self.use_tick))
    }
    pub fn backlog_ticks(&self) -> u128 {
        self.elapsed.as_nanos() / simulation_clock::TICK_DURATION.as_nanos()
    }

    /// Each accepted request remains buffered until queued, including taps shorter than a tick.
    pub fn advance(&mut self, state: &WorldState, attack: ActionPhase, hit: Option<Hit>) {
        self.selected = hit;
        if self.using_item() || self.pending.is_some() {
            return;
        }
        if !self.enabled
            || self.scope.is_none()
            || self.authority.is_none()
            || state.awaiting_correction_ack()
            || state.death.is_some()
            || state.vitals.as_ref().is_none_or(|v| v.health <= 0.)
        {
            self.elapsed = Duration::ZERO;
            return;
        }
        let held = state.inventory.held();
        let same_item = |old: &inventory::java::Slot| match (old, held) {
            (None, Some(None)) => true,
            (Some(a), Some(Some(b))) => {
                a.item == b.item && a.added == b.added && a.removed == b.removed
            }
            _ => false,
        };
        if self.mining.as_ref().is_some_and(|m| {
            !attack.held
                || hit.is_none_or(|h| h.block != m.hit.block || h.state != m.hit.state)
                || !same_item(&m.item)
        }) {
            self.cancel_mining();
            return;
        }
        let Some(hit) = hit else {
            self.elapsed = Duration::ZERO;
            return;
        };
        let Some(held) = held else {
            self.blocked = Some("Native held inventory is not known".into());
            return;
        };
        self.creative = state.status.mode == Some(world::java::hud::GameMode::Creative)
            && state
                .player_dynamics
                .abilities
                .as_ref()
                .is_some_and(|a| a.creative);
        if !self.creative && state.status.mode != Some(world::java::hud::GameMode::Survival) {
            if attack.pressed || attack.held {
                self.blocked =
                    Some("Native Adventure can_break predicates are not implemented".into());
            }
            self.elapsed = Duration::ZERO;
            return;
        }
        if self.creative {
            match super::mining::creative_allowed(held.as_ref()) {
                Ok(true) => {}
                Ok(false) => {
                    self.elapsed = Duration::ZERO;
                    return;
                }
                Err(e) => {
                    self.blocked = Some(e.into());
                    return;
                }
            }
            if attack.pressed {
                self.request(Some(hit), Kind::Break);
                self.elapsed = Duration::ZERO;
                return;
            }
        }
        if !attack.held {
            self.elapsed = Duration::ZERO;
            return;
        }
        if !self.creative && self.mining.is_none() && self.delay == 0 {
            match self.increment(state, hit) {
                Ok(increment) => {
                    self.request(Some(hit), Kind::Break);
                    // Native >=1 is false for 0/0 (zero hardness and zero break speed).
                    if increment.is_nan() || increment < 1. {
                        self.mining = Some(Mining {
                            hit,
                            item: held.clone(),
                            progress: 0.,
                            started: false,
                        });
                    }
                    self.elapsed = Duration::ZERO;
                }
                Err(e) => {
                    self.blocked = Some(e.into());
                    self.elapsed = Duration::ZERO;
                }
            }
            return;
        }
        for _ in 0..4 {
            if self.elapsed < simulation_clock::TICK_DURATION {
                break;
            }
            self.elapsed -= simulation_clock::TICK_DURATION;
            if self.delay > 0 {
                self.delay -= 1;
                continue;
            }
            if self.creative {
                self.request(Some(hit), Kind::Break);
                break;
            }
            let increment = match self.increment(state, hit) {
                Ok(v) => v,
                Err(e) => {
                    self.blocked = Some(e.into());
                    return;
                }
            };
            if let Some(mining) = self.mining.as_mut() {
                mining.progress += increment;
                if mining.progress >= 1. {
                    self.request(Some(hit), Kind::FinishBreak);
                    break;
                }
            } else {
                self.request(Some(hit), Kind::Break);
                // Native >=1 is false for 0/0 (zero hardness and zero break speed).
                if increment.is_nan() || increment < 1. {
                    self.mining = Some(Mining {
                        hit,
                        item: held.clone(),
                        progress: 0.,
                        started: false,
                    });
                }
                break;
            }
        }
    }
    /// Abort is ordered before a slot/use/target change and retained under backpressure.
    pub fn cancel_mining(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        if let Some(mining) = self.mining.take()
            && mining.started
        {
            self.request(Some(mining.hit), Kind::AbortBreak);
        }
        true
    }
    /// Called at the actual queue boundary. A replacement/unload while backpressured
    /// must not deliver an old START/FINISH against a reused position.
    pub fn validate_mining_target(&mut self, current: Option<u32>) -> bool {
        let stale = self.pending.is_some_and(|r| {
            matches!(r.kind, Kind::Break | Kind::FinishBreak)
                && matches!(r.target,Target::Block(h) if Some(h.state)!=current)
        });
        if stale {
            self.pending = None;
            self.cancel_mining();
        }
        !stale
    }
    pub fn mining_progress(&self) -> Option<(Hit, f32)> {
        self.mining.as_ref().map(|m| (m.hit, m.progress))
    }
    fn increment(&self, state: &WorldState, hit: Hit) -> Result<f32, &'static str> {
        let facts = assets::registry::java::mining::builtin()
            .map_err(|_| "Invalid pinned native mining facts")?
            .state(hit.state)
            .ok_or("Unknown native mining state")?;
        let name = state
            .block_name(hit.state)
            .ok_or("Native mining registry is not installed")?;
        let configuration = &state
            .bootstrap
            .as_ref()
            .ok_or("Native mining configuration is not installed")?
            .configuration;
        // Missing named tags have no synchronized members, just as an explicitly
        // empty registry does. Tool defaults still determine speed/drop correctness;
        // never fabricate vanilla memberships over a server's tag definitions.
        let empty_tags = Default::default();
        let tags = configuration
            .tags
            .get("minecraft:block")
            .unwrap_or(&empty_tags);
        let attribute = |name| {
            state
                .player_dynamics
                .attributes
                .get(name)
                .ok_or("Native mining attribute is not known")?
                .value()
                .map(|v| v as f32)
        };
        let (on_ground, eyes_in_water) =
            self.contact.ok_or("Native mining contact is not known")?;
        let effect = |name| state.player_dynamics.effects.get(name).map(|e| e.amplifier);
        let haste = effect("minecraft:haste")
            .into_iter()
            .chain(effect("minecraft:conduit_power"))
            .max();
        super::mining::progress(
            facts,
            name,
            state
                .inventory
                .held()
                .ok_or("Native held item is not known")?
                .as_ref(),
            tags,
            super::mining::Environment {
                on_ground,
                eyes_in_water,
                efficiency: attribute("minecraft:mining_efficiency")?,
                break_speed: attribute("minecraft:block_break_speed")?,
                submerged_speed: attribute("minecraft:submerged_mining_speed")?,
                haste,
                fatigue: effect("minecraft:mining_fatigue"),
            },
        )
    }
    /// Main-hand block use is server-authoritative. No offhand/air-use fallback is invented.
    pub fn use_on(&mut self, state: &WorldState, action: ActionPhase, hit: Option<Hit>) {
        self.selected = hit;
        self.use_target(
            state,
            action,
            hit.map(Target::Block)
                .ok_or("Native Java air use and offhand fallback are not implemented")
                .map(Some),
        );
    }
    /// Schedules a captured use target through the same sequence and transport queue as blocks.
    /// Ok(None) represents a legitimate cooldown/no-item gate, not a successful action.
    pub fn use_target(
        &mut self,
        state: &WorldState,
        action: ActionPhase,
        target: Result<Option<Target>, &'static str>,
    ) {
        if self.using_item() || self.pending.is_some() {
            return;
        }
        if !self.enabled
            || self.scope.is_none()
            || state.awaiting_correction_ack()
            || state.death.is_some()
            || state.vitals.as_ref().is_none_or(|v| v.health <= 0.)
        {
            self.use_elapsed = Duration::ZERO;
            return;
        }
        if !can_use(state) {
            if action.pressed || action.held {
                self.blocked = Some("Native Java block use requires confirmed inventory and an active Survival/Creative/Adventure world; container UI is incomplete".into());
            }
            self.use_elapsed = Duration::ZERO;
            return;
        }
        let interval = simulation_clock::TICK_DURATION * 4;
        if action.pressed || (action.held && self.use_elapsed >= interval) {
            // Pinned Minecraft.startUseItem rightClickDelay is four native ticks.
            self.use_elapsed = if action.pressed {
                Duration::ZERO
            } else {
                self.use_elapsed - interval
            };
            // Retain the tick remainder, excluding any subsequent transport wait.
            self.use_remainder = self.use_elapsed;
            match target {
                Ok(Some(target)) => {
                    let kind = match target {
                        Target::Block(_) => Kind::UseOn,
                        Target::Air(_) => Kind::UseAir,
                        Target::Entity(_) => Kind::UseEntity,
                    };
                    self.request_target(target, kind);
                }
                Ok(None) => {}
                Err(error) => self.blocked = Some(error.into()),
            }
        } else if !action.held {
            self.use_elapsed = Duration::ZERO;
        }
    }
    fn request(&mut self, hit: Option<Hit>, kind: Kind) -> bool {
        let Some(hit) = hit else {
            return false;
        };
        self.request_target(Target::Block(hit), kind)
    }
    fn request_target(&mut self, target: Target, kind: Kind) -> bool {
        let Some((_, epoch)) = self.scope else {
            return false;
        };
        // Native cancellation is unpredicted (constructor sequence0), unlike starts,
        // finishes and uses. It must not advance the prediction sequence counter.
        let sequence = if matches!(kind, Kind::AbortBreak | Kind::ReleaseUse | Kind::UseEntity) {
            0
        } else {
            let Some(sequence) = self.sequence.checked_add(1) else {
                self.blocked = Some("Native Java interaction sequence exhausted".into());
                return false;
            };
            self.sequence = sequence;
            sequence
        };
        self.pending = Some(Request {
            hand: Hand::Main,
            kind,
            dimension_epoch: epoch,
            correction_sequence: self.correction,
            sequence,
            target,
        });
        true
    }
}

fn can_use(state: &WorldState) -> bool {
    use world::java::hud::GameMode;
    state.inventory.held().is_some()
        && state.inventory.opened.is_none()
        && matches!(
            state.status.mode,
            Some(GameMode::Survival | GameMode::Creative | GameMode::Adventure)
        )
}

#[cfg(test)]
mod tests;
