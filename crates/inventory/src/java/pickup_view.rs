//! Native working view after an admitted pickup. Server snapshots remain untouched.
use super::*;
const MAX_PENDING_PICKUP_WRITES: usize = 128;
impl State {
    pub fn pickup_queue_ready(&self) -> bool {
        self.pickup_writes.len() < MAX_PENDING_PICKUP_WRITES
    }

    pub fn carried_view(&self, container: i32) -> Option<&Slot> {
        self.pickup_cursor_view
            .as_ref()
            .filter(|_| {
                self.pickup_view_identity
                    .is_some_and(|id| id.container == container)
            })
            .or_else(|| self.menu(container)?.carried.as_ref())
    }
    /// Call only after admission into the ordered socket queue, never merely on pointer input.
    /// A write receipt is not server acceptance; subsequent server updates replace this view.
    pub fn pickup_queued(&mut self, plan: pickup::Plan, sequence: u64) -> bool {
        if self.active_identity() != Some(plan.identity)
            || self.menu(plan.identity.container).and_then(|m| m.state) != Some(plan.state)
            || !self.pickup_queue_ready()
            || self.pickup_writes.contains(&sequence)
            || self.menu(plan.identity.container).is_none_or(|menu| {
                plan.slot >= menu.len() || plan.changed.keys().any(|index| *index >= menu.len())
            })
        {
            return false;
        }
        self.pickup_view_identity = Some(plan.identity);
        for (index, stack) in plan.changed {
            let storage = match &self.menu(plan.identity.container).unwrap().slots[index] {
                Binding::Player(storage) => Some(*storage),
                Binding::Owned(_) => None,
            };
            if let Some(storage) = storage {
                self.pickup_player_view.insert(storage, stack);
            } else {
                self.pickup_menu_view.insert(index, stack);
            }
        }
        self.pickup_cursor_view = Some(plan.carried);
        self.pickup_writes.insert(sequence);
        self.revision = self.revision.saturating_add(1);
        true
    }
    pub(super) fn clear_pickup_menu_view(&mut self) {
        self.pickup_menu_view.clear();
        self.pickup_cursor_view = None;
        self.pickup_view_identity = None;
        self.pickup_writes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pickup::{Button, Receipt};
    fn fixture() -> (State, Vec<Slot>) {
        let mut state = State::default();
        state.begin(7, 1);
        let mut slots = vec![None; PLAYER_MENU_SIZE];
        slots[39] = Some(Stack {
            item: "minecraft:wooden_sword".into(),
            count: 1,
            added: [("minecraft:unbreakable".into(), Value::Null)].into(),
            removed: Default::default(),
        });
        state
            .admit(
                7,
                1,
                Update::Content {
                    container: 0,
                    state: 3,
                    slots: slots.clone(),
                    carried: None,
                },
            )
            .unwrap();
        (state, slots)
    }
    fn queue(state: &mut State, slot: usize, sequence: u64) {
        let plan = state
            .plan_pickup(state.active_identity().unwrap(), slot, Button::Left, |_| {
                Ok(1)
            })
            .unwrap();
        assert!(state.pickup_queued(plan, sequence));
    }
    #[test]
    fn rapid_pickup_then_place_uses_working_cursor_and_keeps_authority_separate() {
        let (mut state, mut slots) = fixture();
        let authority = state.authority_revision;
        queue(&mut state, 39, 1);
        assert_eq!(state.slot(0, 39), Some(&None));
        assert!(state.carried_view(0).unwrap().is_some());
        assert_eq!(state.storage[3].value(), Some(&slots[39]));
        queue(&mut state, 9, 2);
        assert_eq!(state.slot(0, 9), Some(&slots[39]));
        assert_eq!(state.carried_view(0), Some(&None));
        assert_eq!(state.authority_revision, authority);
        let identity = state.active_identity().unwrap();
        for sequence in [1, 2] {
            assert!(state.record_receipt(Receipt {
                identity,
                sequence,
                closed: false,
                written: true
            }));
        }
        assert_eq!(
            state.authority_revision, authority,
            "socket writes never confirm server acceptance"
        );
        slots[9] = slots[39].take();
        state
            .admit(
                7,
                1,
                Update::Content {
                    container: 0,
                    state: 4,
                    slots: slots.clone(),
                    carried: None,
                },
            )
            .unwrap();
        assert_eq!(state.slot(0, 9), Some(&slots[9]));
        assert!(state.pickup_player_view.is_empty());
        assert!(state.pickup_cursor_view.is_none());
    }
    #[test]
    fn server_denial_and_unsent_commands_restore_the_observed_inventory() {
        for written in [false, true] {
            let (mut state, slots) = fixture();
            queue(&mut state, 39, 1);
            let identity = state.active_identity().unwrap();
            state.record_receipt(Receipt {
                identity,
                sequence: 1,
                closed: false,
                written,
            });
            if written {
                state
                    .admit(
                        7,
                        1,
                        Update::Content {
                            container: 0,
                            state: 4,
                            slots: slots.clone(),
                            carried: None,
                        },
                    )
                    .unwrap();
            }
            assert_eq!(state.slot(0, 39), Some(&slots[39]));
            assert_eq!(state.carried_view(0), Some(&None));
        }
    }
    #[test]
    fn incremental_authority_replaces_each_view_and_new_world_retires_pending_plans() {
        let (mut state, slots) = fixture();
        queue(&mut state, 39, 1);
        queue(&mut state, 9, 2);
        for (slot, stack) in [(39, None), (9, slots[39].clone())] {
            state
                .admit(
                    7,
                    1,
                    Update::Slot {
                        container: 0,
                        state: 4,
                        slot,
                        stack,
                    },
                )
                .unwrap();
        }
        state.admit(7, 1, Update::Cursor(None)).unwrap();
        assert!(state.pickup_player_view.is_empty());
        let stale = state
            .plan_pickup(state.active_identity().unwrap(), 9, Button::Left, |_| Ok(1))
            .unwrap();
        state.begin(7, 2);
        assert!(!state.pickup_queued(stale, 3));
        assert!(state.pickup_cursor_view.is_none());
    }
}
