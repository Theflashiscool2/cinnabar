//! Applies ordered terrain reconciliation through the existing native player owner.
use super::Movement;
use assets::registry::java::physics::Table;
use client_world::java::WorldState;

impl Movement {
    /// Runs synchronously after one cell restore, before later cells or packets are admitted.
    pub(crate) fn reconcile_candidate(
        &mut self,
        state: &mut WorldState,
        physics: &Table,
        candidate: world::java::Reconciliation,
    ) -> Result<(), String> {
        if let Some(saved) = candidate.player_position {
            let snapped = self
                .0
                .reconcile_block(state, physics, candidate.position, candidate.state.0, saved)
                .inspect_err(|error| self.0.blocked = Some(error.clone()))?;
            self.1.corrected |= snapped;
        }
        Ok(())
    }
    /// Unsupported collision keeps the candidate queued and blocks further movement;
    /// an ACK never authorizes silently skipping native collision rollback.
    pub(crate) fn reconcile_blocks(
        &mut self,
        state: &mut WorldState,
        physics: &Table,
    ) -> Result<bool, String> {
        let mut snapped = false;
        while let Some(candidate) = state.block_reconciliations.front().copied() {
            if let Some(saved) = candidate.player_position {
                snapped |= self.0.reconcile_block(
                    state,
                    physics,
                    candidate.position,
                    candidate.state.0,
                    saved,
                )?;
            }
            state.block_reconciliations.pop_front();
        }
        let mut error = None;
        let resumed = state.resume_block_ack_reconciliation(|state, candidate| {
            self.reconcile_candidate(state, physics, candidate)
                .inspect_err(|failure| {
                    error = Some(failure.clone());
                })
        });
        if !resumed {
            return Err(error.unwrap_or_else(|| {
                "Native block reconciliation is waiting for collision facts".into()
            }));
        }
        Ok(snapped || self.1.corrected)
    }
}
