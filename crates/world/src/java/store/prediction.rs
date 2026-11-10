//! Protocol 777 ClientLevel / BlockStatePredictionHandler reconciliation.
//! The ACK ends a prediction; only the retained server state determines the result.
use super::*;
mod order;
pub(super) use order::Order;

#[derive(Clone, Copy, Debug)]
pub struct RetainedBlock {
    sequence: i32,
    state: BlockStateId,
    player_position: [f64; 3],
}

/// A reverted block may require the existing player simulation to restore its saved
/// position, but only when the player collides with the restored block's native shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reconciliation {
    pub position: [i32; 3],
    pub state: BlockStateId,
    pub changed: bool,
    pub player_position: Option<[f64; 3]>,
}

impl Terrain {
    /// Applies a local mutation using the interaction owner's already allocated sequence.
    /// Repeated mutations retain the first server state and position, advancing only sequence.
    pub fn predict_block(
        &mut self,
        sequence: i32,
        position: [i32; 3],
        state: BlockStateId,
        player_position: [f64; 3],
    ) -> bool {
        if sequence <= 0 || sequence < self.prediction_sequence {
            return false;
        }
        let Some(before) = self.block(position) else {
            return false;
        };
        self.prediction_sequence = sequence;
        if before == state {
            return false;
        }
        self.set_block(position, state);
        self.prediction_order.insert(position);
        self.predictions
            .entry(position)
            .and_modify(|retained| retained.sequence = sequence)
            .or_insert(RetainedBlock {
                sequence,
                state: before,
                player_position,
            });
        true
    }

    /// Server writes update the retained state without exposing it before the sequence ACK.
    pub fn set_server_block(&mut self, position: [i32; 3], state: BlockStateId) {
        if let Some(retained) = self.predictions.get_mut(&position) {
            retained.state = state;
        } else {
            self.set_block(position, state);
        }
    }

    /// Records every allocated prediction sequence, including actions with no terrain mutation.
    pub fn prediction_sequence(&mut self, sequence: i32) {
        self.prediction_sequence = self.prediction_sequence.max(sequence);
    }

    /// A teleport suppresses old player rollback without discarding block reconciliation.
    pub fn prediction_teleport(&mut self) {
        self.last_teleport_sequence = Some(self.prediction_sequence);
    }

    /// Ends all retained mutations through this ACK and publishes server state to terrain.
    /// The caller must resolve collision rollback before the next player simulation step.
    pub fn reconcile_blocks(&mut self, sequence: i32) -> Vec<Reconciliation> {
        if sequence < 0 {
            return Vec::new();
        }
        self.prediction_positions_up_to(sequence)
            .into_iter()
            .filter_map(|position| self.reconcile_block(position, sequence))
            .collect()
    }

    /// Captures native map order without applying future cells before the current collision.
    pub fn prediction_positions_up_to(&self, sequence: i32) -> Vec<[i32; 3]> {
        self.prediction_order
            .positions()
            .filter(|position| self.predictions[position].sequence <= sequence)
            .collect()
    }

    /// Restores one retained cell; the caller performs collision/snap before restoring the next.
    pub fn reconcile_block(&mut self, position: [i32; 3], sequence: i32) -> Option<Reconciliation> {
        if self.predictions.get(&position)?.sequence > sequence {
            return None;
        }
        self.prediction_order.remove(position);
        let retained = self.predictions.remove(&position)?;
        let changed = self
            .block(position)
            .is_some_and(|state| state != retained.state);
        self.set_block(position, retained.state);
        Some(Reconciliation {
            position,
            state: retained.state,
            changed,
            player_position: (changed
                && self
                    .last_teleport_sequence
                    .is_none_or(|last| last < sequence))
            .then_some(retained.player_position),
        })
    }

    /// Retires a locally queued action the transport did not write. This is a local
    /// cancellation, never an ACK or a server rejection; newer predictions survive.
    pub fn discard_prediction(&mut self, sequence: i32) -> usize {
        let positions: Vec<_> = self
            .prediction_order
            .positions()
            .filter(|position| self.predictions[position].sequence == sequence)
            .collect();
        let retired = positions.len();
        for position in positions {
            self.prediction_order.remove(position);
            let retained = self
                .predictions
                .remove(&position)
                .expect("retained prediction");
            self.set_block(position, retained.state);
        }
        retired
    }

    /// Number of retained cells, independent of ACK/action counts.
    pub fn predicted_blocks(&self) -> usize {
        self.predictions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn terrain() -> Terrain {
        let mut terrain = Terrain::default();
        terrain.begin(crate::java::Dimension {
            session: 7,
            epoch: 1,
            name: "minecraft:overworld".into(),
            min_y: -64,
            height: 16,
            has_skylight: true,
        });
        terrain.replace(
            [0, 0],
            Column::new(
                vec![Arc::new(Section {
                    blocks: Palette::uniform(BlockStateId(7), 4096),
                    biomes: Palette::uniform(BiomeId(0), 64),
                })],
                Light {
                    sky: vec![None; 3],
                    block: vec![None; 3],
                },
                vec![],
                vec![],
            ),
        );
        terrain
    }
    #[test]
    fn ack_without_server_write_restores_original_and_invalidates_mesh_publication() {
        let mut terrain = terrain();
        let before = terrain.publication_key([0, 0]).unwrap();
        let position = [1, -60, 1];
        assert!(terrain.predict_block(1, position, BlockStateId(0), [1., -59., 1.]));
        assert!(!terrain.is_current(&before));
        let predicted = terrain.publication_key([0, 0]).unwrap();
        let outcome = terrain.reconcile_blocks(1);
        assert_eq!(terrain.block(position), Some(BlockStateId(7)));
        assert_eq!(outcome[0].player_position, Some([1., -59., 1.]));
        assert!(outcome[0].changed);
        assert!(!terrain.is_current(&predicted));
        assert!(terrain.reconcile_blocks(1).is_empty());
    }
    #[test]
    fn server_write_is_hidden_until_ack_and_equal_state_requires_no_rollback() {
        let mut terrain = terrain();
        let position = [1, -60, 1];
        terrain.predict_block(1, position, BlockStateId(0), [1.; 3]);
        terrain.set_server_block(position, BlockStateId(8));
        assert_eq!(terrain.block(position), Some(BlockStateId(0)));
        terrain.set_server_block(position, BlockStateId(0));
        let outcome = terrain.reconcile_blocks(1);
        assert!(!outcome[0].changed);
        assert_eq!(outcome[0].player_position, None);
        assert_eq!(terrain.block(position), Some(BlockStateId(0)));
    }
    #[test]
    fn repeated_prediction_keeps_first_server_state_and_position_until_latest_sequence() {
        let mut terrain = terrain();
        let position = [1, -60, 1];
        terrain.predict_block(1, position, BlockStateId(0), [1.; 3]);
        terrain.predict_block(3, position, BlockStateId(9), [2.; 3]);
        assert!(terrain.reconcile_blocks(2).is_empty());
        let outcome = terrain.reconcile_blocks(3);
        assert_eq!(outcome[0].state, BlockStateId(7));
        assert_eq!(outcome[0].player_position, Some([1.; 3]));
    }
    #[test]
    fn unwritten_action_does_not_retire_a_newer_prediction_at_the_same_position() {
        let mut terrain = terrain();
        let position = [1, -60, 1];
        terrain.predict_block(1, position, BlockStateId(0), [1.; 3]);
        terrain.predict_block(2, position, BlockStateId(9), [2.; 3]);
        assert_eq!(terrain.discard_prediction(1), 0);
        assert_eq!(terrain.block(position), Some(BlockStateId(9)));
        assert_eq!(terrain.predicted_blocks(), 1);
        assert!(terrain.reconcile_blocks(1).is_empty());
        assert_eq!(terrain.discard_prediction(2), 1);
        assert_eq!(terrain.block(position), Some(BlockStateId(7)));
        assert!(terrain.reconcile_blocks(2).is_empty());
    }
    #[test]
    fn teleport_preserves_blocks_but_suppresses_only_older_player_rollback() {
        let mut terrain = terrain();
        terrain.predict_block(1, [1, -60, 1], BlockStateId(0), [1.; 3]);
        terrain.prediction_sequence(2);
        terrain.prediction_teleport();
        assert_eq!(terrain.predicted_blocks(), 1);
        assert_eq!(terrain.reconcile_blocks(2)[0].player_position, None);
        terrain.predict_block(3, [1, -60, 1], BlockStateId(0), [2.; 3]);
        assert_eq!(
            terrain.reconcile_blocks(3)[0].player_position,
            Some([2.; 3])
        );
        let dimension = terrain.dimension().unwrap().clone();
        terrain.begin(dimension);
        assert_eq!(terrain.predicted_blocks(), 0);
    }
}
