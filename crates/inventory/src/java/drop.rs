//! Native drops are predicted by the vanilla client: successful drops have no slot echo.
//! Keep the last server snapshot intact and replace provisional views on authoritative updates.
use super::*;
impl State {
    pub fn player_slot_view(&self, index: usize) -> Option<&Slot> {
        self.pickup_player_view
            .get(&index)
            .or_else(|| self.drop_predictions.get(&index))
            .or_else(|| self.storage.get(index)?.value())
    }
    pub fn drop_written(&mut self, session: u64, epoch: u64, receipt: DropReceipt) -> bool {
        let DropReceipt {
            sequence,
            wire_revision,
            slot,
            whole_stack,
            written,
        } = receipt;
        if !self.accepts(session, epoch) || self.drop_receipt.is_some_and(|old| sequence <= old) {
            return false;
        }
        self.drop_receipt = Some(sequence);
        if !written
            || self.wire_revision != wire_revision
            || self.selected != Some(slot)
            || self.opened.is_some()
        {
            self.drop_notice = Some("Drop not applied: world or inventory changed");
            return true;
        }
        let Some(mut projected) = self.player_slot_view(slot).cloned() else {
            self.drop_notice = Some("Drop not applied: inventory unknown");
            return true;
        };
        if slot >= HOTBAR_SIZE {
            return false;
        }
        if let Some(stack) = &mut projected {
            if whole_stack || stack.count <= 1 {
                projected = None;
            } else {
                stack.count -= 1;
            }
        }
        self.pickup_player_view.remove(&slot);
        self.drop_predictions.insert(slot, projected);
        self.drop_notice = None;
        self.revision = self.revision.saturating_add(1);
        true
    }
}
/// A local transport receipt, never a server acknowledgement.
#[derive(Clone, Copy, Debug)]
pub struct DropReceipt {
    pub sequence: u64,
    pub wire_revision: u64,
    pub slot: usize,
    pub whole_stack: bool,
    pub written: bool,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> State {
        let mut s = State::default();
        s.begin(7, 1);
        s.admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(Stack {
                    item: "minecraft:dirt".into(),
                    count: 5,
                    added: Default::default(),
                    removed: Default::default(),
                }),
            },
        )
        .unwrap();
        s.wire_revision = 3;
        s
    }
    fn receipt(sequence: u64) -> DropReceipt {
        DropReceipt {
            sequence,
            wire_revision: 3,
            slot: 0,
            whole_stack: false,
            written: true,
        }
    }
    fn count(s: &State) -> u32 {
        s.held().unwrap().as_ref().map_or(0, |s| s.count)
    }
    #[test]
    fn written_drops_project_without_changing_authority_and_reconcile() {
        let mut s = fixture();
        let authority = s.authority_revision;
        assert!(s.drop_written(7, 1, receipt(1)));
        assert_eq!(count(&s), 4);
        assert!(!s.drop_written(7, 1, receipt(1)));
        assert_eq!(count(&s), 4);
        assert!(s.drop_written(7, 1, receipt(2)));
        assert_eq!(count(&s), 3);
        assert_eq!(s.storage[0].value().unwrap().as_ref().unwrap().count, 5);
        assert_eq!(s.authority_revision, authority);
        s.admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 1,
                stack: None,
            },
        )
        .unwrap();
        assert_eq!(count(&s), 3);
        let authoritative = s.storage[0].value().unwrap().clone();
        s.admit(
            7,
            1,
            Update::Slot {
                container: 0,
                state: 0,
                slot: 36,
                stack: authoritative,
            },
        )
        .unwrap();
        assert_eq!(count(&s), 5);
        assert!(s.drop_predictions.is_empty());
    }
    #[test]
    fn rejected_stale_or_replaced_receipts_never_consume_and_retained_player_keeps_view() {
        let mut s = fixture();
        let mut r = receipt(1);
        r.written = false;
        assert!(s.drop_written(7, 1, r));
        assert_eq!(count(&s), 5);
        assert!(!s.drop_written(8, 1, receipt(2)));
        assert_eq!(count(&s), 5);
        r = receipt(2);
        r.wire_revision = 2;
        s.drop_written(7, 1, r);
        assert_eq!(count(&s), 5);
        s.drop_written(7, 1, receipt(3));
        assert_eq!(count(&s), 4);
        s.rebind_player(7, 2);
        assert_eq!(count(&s), 4);
        assert_eq!(s.wire_revision, 3);
        assert!(!s.drop_written(7, 1, receipt(4)));
        r = receipt(4);
        r.whole_stack = true;
        s.drop_written(7, 2, r);
        assert_eq!(count(&s), 0);
        s.begin(7, 3);
        assert_eq!(s.held(), None);
        assert!(s.drop_predictions.is_empty());
    }
    #[test]
    fn full_contents_replace_predictions_and_component_patches_survive_single_drop() {
        let mut s = fixture();
        let mut item = s.held().unwrap().clone().unwrap();
        item.added.insert(
            "minecraft:custom_name".into(),
            serde_json::json!({"text":"Kept"}),
        );
        item.removed.insert("minecraft:lore".into());
        s.admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(item.clone()),
            },
        )
        .unwrap();
        s.drop_written(7, 1, receipt(1));
        assert_eq!(s.held().unwrap().as_ref().unwrap().added, item.added);
        assert_eq!(s.held().unwrap().as_ref().unwrap().removed, item.removed);
        let mut slots = vec![None; 46];
        slots[36] = Some(item);
        s.admit(
            7,
            1,
            Update::Content {
                container: 0,
                state: 0,
                slots,
                carried: None,
            },
        )
        .unwrap();
        assert_eq!(count(&s), 5);
        assert!(s.drop_predictions.is_empty());
        assert_eq!(s.player.as_ref().unwrap().carried, Some(None));
    }
}
