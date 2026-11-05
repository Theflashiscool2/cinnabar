//! Native system text enters the existing chat/HUD stores without Bedrock packet synthesis.
use super::{UiRuntime, presentation::nametags::java::text};
use client_world::java::chat::Message;
use ui::{ChatMessage, ChatMessageKind, TitleDurations};
impl UiRuntime {
    pub fn publish_java_system_message(
        &mut self,
        session: u64,
        message: &Message,
        tick: u64,
        now: u64,
    ) -> bool {
        if self.session.session_id != session || self.session.java_hud.is_none() {
            return false;
        }
        if self.session.java_item_names.is_none() {
            return false;
        }
        let result = text::translated_component_text(&message.content, &|key| {
            self.session
                .java_item_names
                .as_ref()
                .and_then(|names| names.get(key))
                .map(str::to_owned)
        });
        let value = match result {
            Ok(value) => value,
            Err(()) => {
                self.push_local_chat_line(
                    "Java system message uses unsupported text components.".into(),
                    now,
                );
                return true;
            }
        };
        let age = tick.saturating_sub(message.received_tick);
        let received = now.saturating_sub(age.saturating_mul(50));
        if message.overlay {
            // Gui.setOverlayMessage:60 ticks, alpha fades over the last20.
            if age < 60 {
                self.session.hud.set_timed_actionbar(
                    value,
                    message.sequence,
                    received,
                    TitleDurations {
                        fade_in_ticks: 0,
                        stay_ticks: 40,
                        fade_out_ticks: 20,
                    },
                );
            }
        } else {
            // Java admission owns packet ordering; this projection shares the receive
            // store without consuming the unrelated Bedrock FIFO sequence namespace.
            let result = self.session.chat.push_local(ChatMessage {
                fifo_sequence: 0,
                received_millis: received,
                kind: ChatMessageKind::System,
                source: None,
                message: value,
                parameters: std::sync::Arc::from([]),
            });
            if matches!(result, ui::ChatApplyResult::RejectedTooLarge) {
                self.session.gameplay_hud.note_oversized_chat_row();
            }
        }
        true
    }
}
