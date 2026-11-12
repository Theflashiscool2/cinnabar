//! One boundary per existing client-timer tick, including when player movement cannot run.
use super::*;

impl Controller {
    /// Uses the SAME elapsed budget as tick(), never a second heartbeat timer. An unsent
    /// boundary (with or without a pose) blocks further prediction until queue handoff.
    pub fn game_tick(
        &mut self,
        state: &mut WorldState,
        physics: Option<&Table>,
        controls: Controls,
    ) -> bool {
        if self.scope.is_none() {
            self.elapsed = Duration::ZERO;
            return false;
        }
        if self.scope != state.terrain.dimension().map(|d| (d.session, d.epoch)) {
            return false;
        }
        if physics.is_none() || state.awaiting_correction_ack() {
            self.defer_movement_time();
        }
        if self.tick_end_pending || self.pending.is_some() {
            return false;
        }
        if self.deferred_idle_ticks > 0 {
            self.deferred_idle_ticks -= 1;
        } else {
            if self.elapsed < TICK {
                return false;
            }
            let budget = self.elapsed;
            if !physics.is_some_and(|physics| self.tick(state, physics, controls)) {
                // A failed collision transaction does not consume physical time. All
                // elapsed waiting deadlines remain idle even if data becomes ready later.
                self.elapsed = budget;
                self.defer_movement_time();
                self.deferred_idle_ticks -= 1;
            }
        }
        self.tick_end_pending = true;
        self.game_ticks = self.game_ticks.saturating_add(1);
        true
    }

    pub(super) fn defer_movement_time(&mut self) {
        let nanos = self.elapsed.as_nanos();
        self.deferred_idle_ticks = self
            .deferred_idle_ticks
            .saturating_add(nanos / TICK.as_nanos());
        self.elapsed = Duration::from_nanos((nanos % TICK.as_nanos()) as u64);
    }

    pub fn pending_tick_end(&self) -> Option<u64> {
        self.tick_end_pending
            .then(|| self.scope.map(|s| s.1))
            .flatten()
    }

    /// Queue acceptance, not a claim of serialization or server acceptance.
    pub fn tick_queued(&mut self) {
        if self.pending.is_some() {
            self.queued();
        } else {
            self.tick_end_pending = false;
        }
    }
}
