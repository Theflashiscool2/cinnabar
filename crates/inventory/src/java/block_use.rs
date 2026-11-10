//! Native BlockItem consumption joins the existing player working view.
//! Socket receipts retire unsent actions; only server inventory packets replace authority.
use super::*;
#[derive(Clone, Copy, Debug)]
pub enum BlockUseHand {
    Main,
    Off,
}
#[derive(Debug, Serialize)]
struct Action {
    sequence: Option<i32>,
    whole_stack: bool,
    written: Option<bool>,
}
#[derive(Debug, Serialize)]
pub(super) struct Journal {
    baseline: Slot,
    actions: Vec<Action>,
}
fn consume(slot: &mut Slot, whole: bool) {
    if let Some(stack) = slot {
        if whole || stack.count <= 1 {
            *slot = None;
        } else {
            stack.count -= 1;
        }
    }
}
impl Journal {
    fn project(&mut self) -> Slot {
        let prefix = self
            .actions
            .iter()
            .take_while(|action| action.written.is_some())
            .count();
        for action in self.actions.drain(..prefix) {
            if action.written == Some(true) {
                consume(&mut self.baseline, action.whole_stack);
            }
        }
        let mut projected = self.baseline.clone();
        for action in &self.actions {
            if action.written != Some(false) {
                consume(&mut projected, action.whole_stack);
            }
        }
        projected
    }
}
impl State {
    /// Predict one validated successful BlockItem result at queue admission, never an attempt.
    /// The hand, selected storage slot, count and component patch must still match that result.
    pub fn predict_block_use(
        &mut self,
        session: u64,
        epoch: u64,
        sequence: i32,
        hand: BlockUseHand,
        expected: &Stack,
    ) -> bool {
        if !self.accepts(session, epoch)
            || sequence <= self.last_block_use_sequence
            || self.opened.is_some()
        {
            return false;
        }
        let slot = match hand {
            BlockUseHand::Main => match self.selected.filter(|slot| *slot < HOTBAR_SIZE) {
                Some(slot) => slot,
                None => return false,
            },
            BlockUseHand::Off => {
                player_storage_slot(OFFHAND_MENU_SLOT).expect("native offhand binding")
            }
        };
        if expected.count == 0
            || self
                .player_slot_view(slot)
                .and_then(Option::as_ref)
                .is_none_or(|current| current != expected)
            || self.block_uses.values().any(|journal| {
                journal
                    .actions
                    .iter()
                    .any(|action| action.sequence == Some(sequence))
            })
        {
            return false;
        }
        let baseline = self
            .player_slot_view(slot)
            .cloned()
            .expect("known matching slot");
        let journal = self.block_uses.entry(slot).or_insert_with(|| Journal {
            baseline,
            actions: Vec::new(),
        });
        journal.actions.push(Action {
            sequence: Some(sequence),
            whole_stack: false,
            written: None,
        });
        let projected = journal.project();
        self.last_block_use_sequence = sequence;
        self.pickup_player_view.remove(&slot);
        self.world_player_view.insert(slot, projected);
        self.revision = self.revision.saturating_add(1);
        true
    }
    /// Handles a local write result only. ACKs never commit consumption or restore a stack.
    pub fn block_use_written(
        &mut self,
        session: u64,
        epoch: u64,
        sequence: i32,
        written: bool,
    ) -> bool {
        if !self.accepts(session, epoch) {
            return false;
        }
        let Some(slot) = self.block_uses.iter().find_map(|(slot, journal)| {
            journal
                .actions
                .iter()
                .any(|action| action.sequence == Some(sequence))
                .then_some(*slot)
        }) else {
            return false;
        };
        let journal = self.block_uses.get_mut(&slot).expect("pending use journal");
        let action = journal
            .actions
            .iter_mut()
            .find(|action| action.sequence == Some(sequence))
            .expect("pending use");
        action.written = Some(written);
        let projected = journal.project();
        let done = journal.actions.is_empty();
        if !written {
            // Later pickup plans may depend on this unsent decrement. Retire those
            // provisional views through the same resync path as an unwritten pickup.
            self.pickup_player_view.clear();
            self.clear_pickup_menu_view();
        }
        self.world_player_view.insert(slot, projected);
        if done {
            self.block_uses.remove(&slot);
        }
        self.revision = self.revision.saturating_add(1);
        true
    }
    /// Existing serialized drops remain ordered after pending consumption in this same view.
    pub(super) fn append_written_drop(&mut self, slot: usize, whole_stack: bool) {
        if let Some(journal) = self.block_uses.get_mut(&slot) {
            journal.actions.push(Action {
                sequence: None,
                whole_stack,
                written: Some(true),
            });
        }
    }
    /// A retained player keeps written working-view changes across login, never old unsent uses.
    pub(super) fn retire_pending_block_uses(&mut self) {
        let journals = std::mem::take(&mut self.block_uses);
        for (slot, mut journal) in journals {
            for action in &mut journal.actions {
                if action.written.is_none() {
                    action.written = Some(false);
                }
            }
            self.world_player_view.insert(slot, journal.project());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stack(count: u32) -> Stack {
        Stack {
            item: "minecraft:white_wool".into(),
            count,
            added: BTreeMap::new(),
            removed: BTreeSet::new(),
        }
    }
    fn fixture(count: u32) -> State {
        let mut state = State::default();
        state.begin(7, 1);
        for (slot, count) in [(0, count), (1, 5), (40, 2)] {
            state
                .admit(
                    7,
                    1,
                    Update::PlayerSlot {
                        slot,
                        stack: Some(stack(count)),
                    },
                )
                .unwrap();
        }
        state
    }
    fn predict(state: &mut State, sequence: i32) {
        let expected = state.held().unwrap().as_ref().unwrap().clone();
        assert!(state.predict_block_use(7, 1, sequence, BlockUseHand::Main, &expected));
    }
    fn count(state: &State, slot: usize) -> u32 {
        state
            .player_slot_view(slot)
            .unwrap()
            .as_ref()
            .map_or(0, |stack| stack.count)
    }
    #[test]
    fn one_item_is_consumed_once_without_mutating_authority_and_false_receipt_restores_it() {
        let mut state = fixture(1);
        let authority = state.authority_revision;
        predict(&mut state, 1);
        assert_eq!(state.held(), Some(&None));
        assert!(!state.predict_block_use(7, 1, 2, BlockUseHand::Main, &stack(1)));
        assert_eq!(state.storage[0].value(), Some(&Some(stack(1))));
        assert_eq!(state.authority_revision, authority);
        assert!(state.block_use_written(7, 1, 1, false));
        assert_eq!(count(&state, 0), 1);
        assert!(!state.block_use_written(7, 1, 1, false));
        assert!(!state.predict_block_use(7, 1, 1, BlockUseHand::Main, &stack(1)));
    }
    #[test]
    fn dropping_earlier_sequence_recomputes_newer_consumption_and_written_is_not_acceptance() {
        let mut state = fixture(3);
        predict(&mut state, 1);
        predict(&mut state, 2);
        assert_eq!(count(&state, 0), 1);
        assert!(state.block_use_written(7, 1, 1, false));
        assert_eq!(count(&state, 0), 2);
        assert!(state.block_use_written(7, 1, 2, true));
        assert_eq!(count(&state, 0), 2);
        assert_eq!(state.storage[0].value(), Some(&Some(stack(3))));
        state
            .admit(
                7,
                1,
                Update::PlayerSlot {
                    slot: 0,
                    stack: Some(stack(3)),
                },
            )
            .unwrap();
        assert_eq!(
            count(&state, 0),
            3,
            "server denial replaces the working view"
        );
    }
    #[test]
    fn slot_switch_offhand_component_replacement_and_reconnect_retire_only_their_ownership() {
        let mut state = fixture(3);
        predict(&mut state, 1);
        state.admit(7, 1, Update::Selected(1)).unwrap();
        assert_eq!(count(&state, 0), 2);
        assert_eq!(state.held(), Some(&Some(stack(5))));
        assert!(state.predict_block_use(7, 1, 2, BlockUseHand::Off, &stack(2)));
        assert_eq!(count(&state, 40), 1);
        assert_eq!(count(&state, 1), 5);
        let mut replacement = stack(9);
        replacement.added.insert(
            "minecraft:custom_name".into(),
            serde_json::json!("new component"),
        );
        state
            .admit(
                7,
                1,
                Update::PlayerSlot {
                    slot: 0,
                    stack: Some(replacement.clone()),
                },
            )
            .unwrap();
        assert!(!state.block_use_written(7, 1, 1, false));
        assert_eq!(state.player_slot_view(0), Some(&Some(replacement)));
        assert!(!state.block_use_written(7, 2, 2, false));
        assert_eq!(count(&state, 40), 1);
        state.begin(8, 1);
        assert!(state.block_uses.is_empty());
        assert!(state.world_player_view.is_empty());
        assert!(!state.block_use_written(7, 1, 2, false));
        assert_eq!(state.held(), None);
    }
    #[test]
    fn serialized_drop_stays_ordered_when_an_earlier_use_was_not_written() {
        let mut state = fixture(5);
        predict(&mut state, 1);
        assert!(state.drop_written(
            7,
            1,
            super::super::DropReceipt {
                sequence: 1,
                wire_revision: state.wire_revision,
                slot: 0,
                whole_stack: false,
                written: true,
            }
        ));
        assert_eq!(count(&state, 0), 3);
        assert!(state.block_use_written(7, 1, 1, false));
        assert_eq!(
            count(&state, 0),
            4,
            "only the written drop remains consumed"
        );
        predict(&mut state, 2);
        assert!(state.drop_written(
            7,
            1,
            super::super::DropReceipt {
                sequence: 2,
                wire_revision: state.wire_revision,
                slot: 0,
                whole_stack: true,
                written: true,
            }
        ));
        assert!(state.block_use_written(7, 1, 2, false));
        assert_eq!(
            state.held(),
            Some(&None),
            "a whole-stack drop cannot resurrect the old stack"
        );
    }
    #[test]
    fn retained_player_keeps_written_consumption_but_not_old_epoch_pending_uses() {
        let mut state = fixture(5);
        predict(&mut state, 1);
        state.block_use_written(7, 1, 1, true);
        predict(&mut state, 2);
        state.rebind_player(7, 2);
        assert_eq!(count(&state, 0), 4);
        assert!(state.block_uses.is_empty());
        assert!(!state.block_use_written(7, 1, 2, false));
        assert!(state.predict_block_use(7, 2, 1, BlockUseHand::Main, &stack(4)));
        assert_eq!(count(&state, 0), 3);
    }
}
