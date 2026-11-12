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
        if self.tick_end_pending || self.pending.is_some() || self.elapsed < TICK {
            return false;
        }
        let budget = self.elapsed;
        if !physics.is_some_and(|physics| self.tick(state, physics, controls)) {
            // tick() can discard kinematic backlog after missing collision facts. Those
            // are still logical game ticks; consume exactly one, without moving the body.
            self.elapsed = budget - TICK;
        }
        self.tick_end_pending = true;
        self.game_ticks = self.game_ticks.saturating_add(1);
        true
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
