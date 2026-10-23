//! Native block intent. Shared action sequences never predict terrain or stack changes.
use client_world::java::WorldState;
use semantic_input::{ActionPhase, identity::InputAuthorityGeneration};
use sim::java::selection::Hit;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Break,
    UseOn,
}
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub kind: Kind,
    pub dimension_epoch: u64,
    pub correction_sequence: u64,
    pub sequence: i32,
    pub hit: Hit,
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
    pub selected: Option<Hit>,
    pub last_queued: Option<Request>,
    pub queued_actions: u64,
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
            self.pending = None;
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
        let invalid_break = !matches!(state.inventory.held(), Some(None))
            || state.status.mode != Some(world::java::hud::GameMode::Creative)
            || !state
                .player_dynamics
                .abilities
                .as_ref()
                .is_some_and(|a| a.creative);
        if self.pending.is_some_and(|request| match request.kind {
            Kind::Break => invalid_break,
            Kind::UseOn => !can_use(state),
        }) || state.death.is_some()
            || state.vitals.as_ref().is_none_or(|v| v.health <= 0.)
        {
            self.pending = None;
            self.elapsed = Duration::ZERO;
            self.use_elapsed = Duration::ZERO;
        }
        self.blocked = None;
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
                    self.delay = 5; // Pinned MultiPlayerGameMode creative destroyDelay, in ticks.
                    self.elapsed = Duration::ZERO;
                }
                Kind::UseOn => self.use_elapsed = self.use_remainder,
            }
        }
    }
    pub fn backlog_ticks(&self) -> u128 {
        self.elapsed.as_nanos() / simulation_clock::TICK_DURATION.as_nanos()
    }

    /// Each accepted request remains buffered until queued, including taps shorter than a tick.
    pub fn advance(&mut self, state: &WorldState, attack: ActionPhase, hit: Option<Hit>) {
        self.selected = hit;
        if self.pending.is_some() {
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
        let creative = state
            .player_dynamics
            .abilities
            .as_ref()
            .is_some_and(|a| a.creative)
            && state.status.mode == Some(world::java::hud::GameMode::Creative);
        if !creative {
            if attack.pressed || attack.held {
                self.blocked = Some("Native Java survival/tool breaking is not implemented".into());
            }
            self.elapsed = Duration::ZERO;
            return;
        }
        if !matches!(state.inventory.held(), Some(None)) {
            if attack.pressed || attack.held {
                self.blocked = Some("Native Java breaking currently requires a confirmed empty hand; item/tool behavior is incomplete".into());
            }
            self.elapsed = Duration::ZERO;
            return;
        }
        if attack.pressed {
            self.request(hit, Kind::Break);
            // The initial click is this tick's action; do not repeat it from accumulated time.
            self.elapsed = Duration::ZERO;
            return;
        }
        if !attack.held {
            self.elapsed = Duration::ZERO;
            return;
        }
        for _ in 0..4 {
            if self.elapsed < simulation_clock::TICK_DURATION {
                break;
            }
            self.elapsed -= simulation_clock::TICK_DURATION;
            if !attack.held {
                continue;
            }
            if self.delay > 0 {
                self.delay -= 1;
                continue;
            }
            if self.request(hit, Kind::Break) {
                break;
            }
        }
    }
    /// Main-hand block use is server-authoritative. No offhand/air-use fallback is invented.
    pub fn use_on(&mut self, state: &WorldState, action: ActionPhase, hit: Option<Hit>) {
        self.selected = hit;
        if self.pending.is_some() {
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
            if !self.request(hit, Kind::UseOn) && hit.is_none() {
                self.blocked =
                    Some("Native Java air use and offhand fallback are not implemented".into());
            }
        } else if !action.held {
            self.use_elapsed = Duration::ZERO;
        }
    }
    fn request(&mut self, hit: Option<Hit>, kind: Kind) -> bool {
        let Some(hit) = hit else {
            return false;
        };
        let Some((_, epoch)) = self.scope else {
            return false;
        };
        let Some(sequence) = self.sequence.checked_add(1) else {
            self.blocked = Some("Native Java interaction sequence exhausted".into());
            return false;
        };
        self.sequence = sequence;
        self.pending = Some(Request {
            kind,
            dimension_epoch: epoch,
            correction_sequence: self.correction,
            sequence,
            hit,
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
