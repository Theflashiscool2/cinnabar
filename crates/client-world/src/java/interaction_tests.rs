use super::*;

#[test]
fn block_receipts_are_scoped_processing_facts_and_never_change_terrain() {
    let mut state = tests_support::state();
    let epoch = state.terrain.dimension().unwrap().epoch;
    let receive = |generation, dimension_epoch, sequence| Envelope {
        generation,
        event: Event::BlockAck {
            dimension_epoch,
            sequence,
        },
    };
    assert!(!state.admit(receive(6, epoch, 1)));
    assert!(!state.admit(receive(7, epoch + 1, 1)));
    assert!(!state.admit(receive(7, epoch, -1)));
    assert_eq!(state.block_ack, None);
    let count = state.terrain.len();
    let updates = state.block_updates;
    assert!(state.admit(receive(7, epoch, 4)));
    assert_eq!(state.block_ack, Some((epoch, 4)));
    assert_eq!(state.terrain.len(), count);
    assert_eq!(state.block_updates, updates);
    let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    terrain::begin(&mut state, &spawn).unwrap();
    assert_eq!(state.block_ack, None);
    assert!(!state.admit(receive(7, epoch, 5)));
    state.begin(8);
    assert_eq!(state.block_ack, None);
}

#[test]
fn native_block_prediction_admission_separates_server_updates_ack_and_local_write_receipts() {
    use std::sync::Arc;
    use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
    let mut state = tests_support::state();
    let epoch = state.terrain.dimension().unwrap().epoch;
    let section = Arc::new(Section {
        blocks: Palette::uniform(BlockStateId(7), 4096),
        biomes: Palette::uniform(BiomeId(0), 64),
    });
    assert!(state.terrain.replace(
        [0, 0],
        Column::new(
            vec![section; 24],
            Light {
                sky: vec![None; 26],
                block: vec![None; 26]
            },
            vec![],
            vec![]
        )
    ));
    let position = [1, -60, 1];
    assert!(
        state
            .terrain
            .predict_block(1, position, BlockStateId(0), [1.; 3])
    );
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::Terrain {
            dimension_epoch: epoch,
            packet: Box::new(java_protocol::terrain::Packet::Blocks(vec![
                java_protocol::terrain::BlockUpdate { position, state: 9 }
            ])),
        },
    }));
    assert_eq!(state.terrain.block(position), Some(BlockStateId(0)));
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::BlockActionWritten {
            dimension_epoch: epoch,
            sequence: 1,
            written: true,
        }
    }));
    assert_eq!(state.terrain.block(position), Some(BlockStateId(0)));
    assert!(state.block_reconciliations.is_empty());
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::BlockAck {
            dimension_epoch: epoch,
            sequence: 1,
        }
    }));
    assert_eq!(state.terrain.block(position), Some(BlockStateId(9)));
    assert_eq!(state.block_predictions_reverted, 1);
    assert_eq!(
        state.block_reconciliations.front().unwrap().player_position,
        Some([1.; 3])
    );
    assert!(
        state
            .terrain
            .predict_block(2, position, BlockStateId(0), [1.; 3])
    );
    assert!(
        state
            .terrain
            .predict_block(3, position, BlockStateId(4), [1.; 3])
    );
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::BlockActionWritten {
            dimension_epoch: epoch,
            sequence: 2,
            written: false,
        }
    }));
    assert_eq!(state.terrain.block(position), Some(BlockStateId(4)));
    assert_eq!(state.block_predictions_retired, 0);
    assert!(state.admit(Envelope {
        generation: 7,
        event: Event::BlockActionWritten {
            dimension_epoch: epoch,
            sequence: 3,
            written: false,
        }
    }));
    assert_eq!(state.terrain.block(position), Some(BlockStateId(9)));
    assert_eq!(state.block_predictions_retired, 1);
    assert_eq!(state.block_predictions_confirmed, 0);
}

#[test]
fn ack_collision_callback_observes_each_restore_before_later_cells_change() {
    use std::sync::Arc;
    use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
    let mut state = tests_support::state();
    let section = Arc::new(Section {
        blocks: Palette::uniform(BlockStateId(7), 4096),
        biomes: Palette::uniform(BiomeId(0), 64),
    });
    assert!(state.terrain.replace(
        [0, 0],
        Column::new(
            vec![section; 24],
            Light {
                sky: vec![None; 26],
                block: vec![None; 26]
            },
            vec![],
            vec![]
        )
    ));
    for position in [[1, -60, 1], [2, -60, 1]] {
        assert!(
            state
                .terrain
                .predict_block(1, position, BlockStateId(0), [1.; 3])
        );
    }
    let positions = state.terrain.prediction_positions_up_to(1);
    let mut observed = 0;
    assert!(state.admit_with_block_reconciliation(
        Envelope {
            generation: 7,
            event: Event::BlockAck {
                dimension_epoch: 1,
                sequence: 1
            },
        },
        |state, candidate| {
            assert_eq!(candidate.position, positions[observed]);
            assert_eq!(
                state.terrain.block(candidate.position),
                Some(BlockStateId(7))
            );
            if observed == 0 {
                assert_eq!(
                    state.terrain.block(positions[1]),
                    Some(BlockStateId(0)),
                    "collision must see later entries still predicted"
                );
            } else {
                assert_eq!(state.terrain.block(positions[0]), Some(BlockStateId(7)));
            }
            observed += 1;
            Ok(())
        }
    ));
    assert_eq!(observed, 2);
    assert!(state.block_reconciliations.is_empty());
}
