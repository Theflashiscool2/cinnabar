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
