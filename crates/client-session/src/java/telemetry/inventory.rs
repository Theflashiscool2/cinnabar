//! Numeric admission context only. Never retains item components, titles or cooldown groups.
use super::{RECENT, Recorder};
use java_protocol::session::inventory::Packet;
use serde_json::{Value, json};

/// Context captured before admission; error presence afterwards is an observation, not a cause.
#[derive(Debug)]
pub struct Refusal {
    packet: &'static str,
    items: Vec<Option<(u32, u32)>>,
    items_truncated: bool,
    container: Option<i32>,
    slot: Option<i32>,
    slots: Option<usize>,
    revision: Option<i32>,
    scope_matches: bool,
    incarnation: u64,
    current_incarnation: u64,
    active_container: Option<i32>,
}
impl Refusal {
    pub fn observe(
        packet: &Packet,
        scope_matches: bool,
        incarnation: u64,
        current_incarnation: u64,
        active_container: Option<i32>,
    ) -> Self {
        // Diagnostic sample budget only; never truncates or rejects gameplay admission.
        const ITEM_SAMPLES: usize = 128;
        let (items, items_truncated) = match packet {
            Packet::Content { slots, .. } => (
                slots
                    .iter()
                    .take(ITEM_SAMPLES)
                    .map(|s| s.as_ref().map(|s| (s.item, s.count)))
                    .collect(),
                slots.len() > ITEM_SAMPLES,
            ),
            Packet::Slot { stack, .. }
            | Packet::PlayerSlot { stack, .. }
            | Packet::Cursor(stack) => (vec![stack.as_ref().map(|s| (s.item, s.count))], false),
            _ => (Vec::new(), false),
        };
        let (packet, container, slot, slots, revision) = match packet {
            Packet::Content {
                container,
                state,
                slots,
                ..
            } => (
                "content",
                Some(*container),
                None,
                Some(slots.len()),
                Some(*state),
            ),
            Packet::Slot {
                container,
                state,
                slot,
                ..
            } => (
                "slot",
                Some(*container),
                Some(i32::from(*slot)),
                None,
                Some(*state),
            ),
            Packet::PlayerSlot { slot, .. } => ("player_slot", None, Some(*slot), None, None),
            Packet::Cursor(_) => ("cursor", None, None, None, None),
            Packet::Selected(slot) => ("selected", None, Some(*slot), None, None),
            Packet::Open { container, .. } => ("open", Some(*container), None, None, None),
            Packet::Close(container) => ("close", Some(*container), None, None, None),
            Packet::Property { container, .. } => ("property", Some(*container), None, None, None),
            Packet::Cooldown { .. } => ("cooldown", None, None, None, None),
            Packet::MerchantOffers(summary) => {
                ("merchant", Some(summary.container), None, None, None)
            }
        };
        Self {
            packet,
            items,
            items_truncated,
            container,
            slot,
            slots,
            revision,
            scope_matches,
            incarnation,
            current_incarnation,
            active_container,
        }
    }
    fn value(self, inventory_error_present: bool) -> Value {
        json!({"packet":self.packet,"items":self.items,"items_truncated":self.items_truncated,
            "container":self.container,"slot":self.slot,
            "slots":self.slots,"revision":self.revision,"scope_matches":self.scope_matches,
            "incarnation":self.incarnation,"current_incarnation":self.current_incarnation,
            "active_container":self.active_container,"inventory_error_present":inventory_error_present})
    }
}
impl Recorder {
    /// The caller counts the refusal separately; this bounded history adds no admission policy.
    pub fn inventory_refused(&self, context: Refusal, inventory_error_present: bool) {
        let mut state = self.state();
        if state.finished {
            return;
        }
        if state.recent_inventory_refusals.len() == RECENT {
            state.recent_inventory_refusals.pop_front();
        }
        state
            .recent_inventory_refusals
            .push_back(context.value(inventory_error_present));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refusals_retain_numeric_scope_context_without_item_payloads_and_are_bounded() {
        let recorder = Recorder::configured(Some("1")).unwrap();
        let packet = Packet::Content {
            container: 3,
            state: 7,
            slots: vec![Some(java_protocol::session::inventory::Stack {
                item: 1,
                count: 1,
                added: [("minecraft:custom_name", json!("private account chat token"))].into(),
                removed: Default::default(),
            })],
            carried: None,
        };
        for i in 0..RECENT + 5 {
            recorder
                .inventory_refused(Refusal::observe(&packet, false, i as u64, 3, Some(4)), true);
        }
        let snapshot = recorder.snapshot();
        let history = snapshot["recent_inventory_refusals"].as_array().unwrap();
        assert_eq!(history.len(), RECENT);
        assert_eq!(history[0]["incarnation"], 5);
        assert_eq!(history[0]["slots"], 1);
        assert_eq!(history[0]["active_container"], 4);
        assert_eq!(history[0]["scope_matches"], false);
        assert_eq!(history[0]["inventory_error_present"], true);
        assert!(!snapshot.to_string().contains("private account chat token"));
        assert!(!snapshot.to_string().contains("custom_name"));
    }
}
