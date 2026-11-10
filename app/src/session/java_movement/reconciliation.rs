//! Applies ordered terrain reconciliation through the existing native player owner.
use super::Movement;
use assets::registry::java::physics::Table;
use client_world::java::WorldState;

impl Movement {
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
        Ok(snapped)
    }
}
