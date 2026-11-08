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
        let lookup = |key: &str| {
            self.session
                .java_item_names
                .as_ref()
                .and_then(|names| names.get(key))
                .map(str::to_owned)
        };
        let strict = text::translated_component_text(&message.content, &lookup);
        let (value, degraded) = match strict {
            Ok(value) => (value, false),
            Err(()) => match text::translated_component_content(&message.content, &lookup) {
                Ok(value) => (value, true),
                Err(()) => {
                    tracing::warn!(target: "java_session", sequence=message.sequence,
                        "Native Java chat content cannot be projected within supported component limits");
                    self.push_local_chat_line(
                        "Java message content is unsupported; see the client log.".into(),
                        now,
                    );
                    return true;
                }
            },
        };
        if degraded && !self.session.java_chat_style_notice {
            self.session.java_chat_style_notice = true;
            tracing::warn!(target: "java_session",
                "Native Java chat presentation is incomplete; retaining message text without unsupported styling");
            self.push_local_chat_line(
                "Some Java message styling is unsupported; message text is retained.".into(),
                now,
            );
        }
        let age = tick.saturating_sub(message.received_tick);
        let received = now.saturating_sub(age.saturating_mul(50));
        if message.overlay {
            // Hud.setOverlayMessage:60 ticks, alpha fades over the last20.
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

#[cfg(test)]
mod tests {
    use super::*;
    use world::{NbtCompound, NbtValue, java::hud::Snapshot};
    #[test]
    fn actual_message_survives_style_gap_and_notice_is_bounded_to_the_session() {
        let mut ui = UiRuntime::new(7);
        ui.session.java_hud = Some(Snapshot {
            session: 7,
            epoch: 1,
            mode: None,
            hardcore: false,
            selected_slot: None,
            health: None,
            maximum_health: None,
            food: None,
            saturation: None,
            armor: None,
            experience: None,
        });
        ui.session.java_item_names = Some(std::sync::Arc::new(
            assets::registry::java::language::Language::load(b"{}").unwrap(),
        ));
        let mut component = NbtCompound::default();
        component.insert(
            "text",
            NbtValue::String("You cannot send messages here.".into()),
        );
        component.insert("color", NbtValue::String("#12ABEF".into()));
        for sequence in 1..=2 {
            assert!(ui.publish_java_system_message(
                7,
                &Message {
                    sequence,
                    content: NbtValue::Compound(component.clone()),
                    overlay: false,
                    received_tick: 20,
                },
                22,
                2000
            ));
        }
        let messages = ui.session.chat.messages();
        assert_eq!(
            messages.len(),
            3,
            "one notice and both real server messages"
        );
        assert_eq!(
            &*messages.back().unwrap().message,
            "§rYou cannot send messages here."
        );
        assert_eq!(messages.back().unwrap().received_millis, 1900);
        assert!(!ui.publish_java_system_message(
            8,
            &Message {
                sequence: 3,
                content: NbtValue::String("retired".into()),
                overlay: false,
                received_tick: 20,
            },
            22,
            2000
        ));
        ui.begin_session(8);
        assert!(!ui.session.java_chat_style_notice);
    }
}
