//! Correlates native server completions with the existing editor and suggestion list.
use super::UiRuntime;
use std::{ops::Range, sync::Arc};
use ui::{ChatAutocompleteApply, ChatAutocompleteRequest, ChatAutocompleteResponse};

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    epoch: Option<u64>,
    next: i32,
    pub pending: Option<(i32, ChatAutocompleteRequest, bool)>,
    pub range: Option<Range<usize>>,
}
impl UiRuntime {
    /// Retains the most recent edit until the one native command queue accepts it.
    pub fn java_completion_request(&mut self, epoch: u64) -> Option<(i32, String)> {
        if self.session.java_completion.epoch != Some(epoch) {
            let replacing = self.session.java_completion.epoch.is_some();
            self.session.java_completion.epoch = Some(epoch);
            self.session.java_completion.pending = None;
            self.session.java_completion.range = None;
            if replacing {
                self.session.chat_autocomplete.clear();
                self.note_chat_editor_change();
            }
        }
        let state = &mut self.session.java_completion;
        if !self.session.chat_focused {
            state.pending = None;
            state.range = None;
            return None;
        }
        if let Some(request) = self.session.pending_chat_autocomplete_request.take() {
            state.pending = None;
            state.range = None;
            let head = &request.input[..usize::from(request.cursor_byte)];
            if head.starts_with('/') {
                let id = state.next;
                state.next = state.next.wrapping_add(1);
                state.pending = Some((id, request, false));
            }
        }
        let (id, request, sent) = state.pending.as_ref()?;
        (!sent).then(|| {
            (
                *id,
                request.input[..usize::from(request.cursor_byte)].to_owned(),
            )
        })
    }
    pub fn mark_java_completion_sent(&mut self, id: i32) {
        if let Some((pending, _, sent)) = &mut self.session.java_completion.pending
            && *pending == id
        {
            *sent = true;
        }
    }
    /// Rejects delayed responses after edits, closes or world replacement. Server ranges
    /// are UTF-16 positions; neither a surrogate midpoint nor an out-of-input range is valid.
    pub fn publish_java_completion(
        &mut self,
        epoch: u64,
        response: &client_world::java::chat::Suggestions,
    ) -> bool {
        let state = &mut self.session.java_completion;
        if state.epoch != Some(epoch) || !self.session.chat_focused {
            return false;
        }
        let Some((id, request, true)) = state.pending.as_ref() else {
            return false;
        };
        if *id != response.id {
            return false;
        }
        let Some(range) = byte_range(&request.input, response.start, response.length) else {
            state.pending = None;
            return false;
        };
        if range.end > usize::from(request.cursor_byte) {
            state.pending = None;
            return false;
        }
        let result = self
            .session
            .chat_autocomplete
            .apply_response(ChatAutocompleteResponse {
                session: request.session,
                input_revision: request.input_revision,
                request_id: request.request_id,
                catalog_revision: 0,
                suggestions: response
                    .entries
                    .iter()
                    .map(|(text, _)| Arc::from(text.as_str()))
                    .collect(),
            });
        state.pending = None;
        if result.is_err() {
            self.session.chat_usage_hint = Some(Arc::from(
                "Server suggestions exceed this editor’s current presentation budget.",
            ));
        }
        if matches!(result, Ok(ChatAutocompleteApply::Applied)) {
            state.range = Some(range);
            true
        } else {
            false
        }
    }
}
fn byte_range(input: &str, start: i32, length: i32) -> Option<Range<usize>> {
    let start = usize::try_from(start).ok()?;
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    fn boundary(input: &str, target: usize) -> Option<usize> {
        let mut units = 0;
        for (byte, ch) in input.char_indices() {
            if units == target {
                return Some(byte);
            }
            units += ch.len_utf16();
        }
        (units == target).then_some(input.len())
    }
    Some(boundary(input, start)?..boundary(input, end)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opened() -> UiRuntime {
        let mut runtime = UiRuntime::new(7);
        runtime.open_chat(&mut player_state::PlayerState::new(7));
        runtime
    }
    fn response(
        id: i32,
        start: i32,
        length: i32,
        texts: &[&str],
    ) -> client_world::java::chat::Suggestions {
        client_world::java::chat::Suggestions {
            id,
            start,
            length,
            entries: texts.iter().map(|s| (s.to_string(), None)).collect(),
        }
    }
    #[test]
    fn responses_follow_input_identity_and_preserve_server_ranges_when_cycling() {
        let mut runtime = opened();
        runtime.insert_chat_text("/msg 夜😀 tail").unwrap();
        runtime.mutate_chat_editor(|editor| editor.place_cursor(12));
        let (id, input) = runtime.java_completion_request(1).unwrap();
        assert_eq!(input, "/msg 夜😀");
        // A full queue retains the exact request rather than losing the keystroke.
        assert_eq!(runtime.java_completion_request(1), Some((id, input)));
        runtime.mark_java_completion_sent(id);
        assert!(runtime.java_completion_request(1).is_none());
        assert!(
            runtime.publish_java_completion(1, &response(id, 5, 3, &["Alice One", "Alice Two"]))
        );
        assert!(runtime.handle_chat_ui_action(ui::UiAction::TabNext));
        assert_eq!(runtime.chat_editor().as_str(), "/msg Alice One tail");
        assert!(runtime.handle_chat_ui_action(ui::UiAction::TabNext));
        assert_eq!(runtime.chat_editor().as_str(), "/msg Alice Two tail");
    }
    #[test]
    fn edits_closes_and_dimension_replacement_reject_old_responses() {
        let mut runtime = opened();
        runtime.insert_chat_text("/he").unwrap();
        let (old, _) = runtime.java_completion_request(1).unwrap();
        runtime.mark_java_completion_sent(old);
        runtime.insert_chat_text("l").unwrap();
        assert!(!runtime.publish_java_completion(1, &response(old, 1, 2, &["help"])));
        let (id, _) = runtime.java_completion_request(1).unwrap();
        assert_ne!(id, old);
        runtime.mark_java_completion_sent(id);
        runtime.close_chat();
        runtime.open_chat(&mut player_state::PlayerState::new(7));
        runtime.insert_chat_text("/he").unwrap();
        let (next, _) = runtime.java_completion_request(1).unwrap();
        assert_ne!(id, next);
        runtime.mark_java_completion_sent(next);
        assert!(!runtime.publish_java_completion(1, &response(id, 1, 2, &["help"])));
        assert!(runtime.publish_java_completion(1, &response(next, 1, 2, &["help"])));
        assert_eq!(runtime.chat_suggestions().len(), 1);
        let (replacement, _) = runtime.java_completion_request(2).unwrap();
        assert_ne!(replacement, next);
        assert!(runtime.chat_suggestions().is_empty());
        assert!(!runtime.publish_java_completion(2, &response(next, 1, 2, &["help"])));
    }
    #[test]
    fn invalid_surrogate_ranges_never_edit_or_publish() {
        let mut runtime = opened();
        runtime.insert_chat_text("/msg 😀").unwrap();
        let (id, _) = runtime.java_completion_request(1).unwrap();
        runtime.mark_java_completion_sent(id);
        assert!(!runtime.publish_java_completion(1, &response(id, 6, 1, &["Alice"])));
        assert_eq!(runtime.chat_editor().as_str(), "/msg 😀");
        assert!(runtime.chat_suggestions().is_empty());
    }
    #[test]
    fn utf16_ranges_preserve_cjk_supplementary_and_untouched_tail() {
        assert_eq!(byte_range("/msg 夜😀 tail", 5, 3), Some(5..12));
        assert_eq!(byte_range("/msg 夜😀 tail", 7, 1), None);
        assert_eq!(byte_range("/he", 1, 2), Some(1..3));
        assert_eq!(byte_range("/he", -1, 1), None);
        assert_eq!(byte_range("/he", 1, i32::MAX), None);
    }
}
