//! Native connection/command adapter for the existing shared chat editor and FIFO.
use super::SessionController;
use client_ui::ui_runtime::UiRuntime;
use java_protocol::session::{Command, Phase};
#[derive(Default)]
pub(crate) struct Projection {
    generation: u64,
    seen: u64,
}
impl Projection {
    pub(super) fn publish(
        &mut self,
        world: &client_world::java::WorldState,
        ui: &mut UiRuntime,
        now: u64,
    ) {
        if self.generation != world.generation() {
            *self = Self {
                generation: world.generation(),
                seen: 0,
            };
        }
        let seen = self.seen;
        for message in world.chat.messages.iter().chain(world.chat.overlay.iter()) {
            if message.sequence > seen
                && ui.publish_java_system_message(
                    world.generation(),
                    message,
                    world.inventory.cooldowns.client_tick(),
                    now,
                )
            {
                self.seen = self.seen.max(message.sequence);
            }
        }
    }
}
/// Owns Java sends exclusively; no Java request is projected into a Bedrock packet.
pub(crate) fn flush(controller: &mut SessionController, ui: &mut UiRuntime, now: u64) -> bool {
    let Some(java) = controller.java.as_ref() else {
        return ui.java_hud().is_some();
    };
    let state = &controller.java_world;
    if let Some((sequence, written)) = state.chat.written {
        if ui.acknowledge_chat_send(state.generation(), sequence) && !written {
            ui.push_local_chat_line(
                "Java command was not sent because the world changed.".into(),
                now,
            );
        }
    }
    if ui.in_flight_chat_sends() != 0 || state.phase != Some(Phase::Play) {
        return true;
    }
    let Some(request) = ui.pending_chat_sends().front().cloned() else {
        return true;
    };
    if request.session != state.generation() {
        return true;
    }
    let result = prepare(&request.message);
    let command = match result {
        Ok(command) => command,
        Err(error) => {
            ui.push_local_chat_line(error.into(), now);
            ui.confirm_chat_send(request.sequence);
            return true;
        }
    };
    let Some(dimension) = state.terrain.dimension() else {
        return true;
    };
    match java.command(Command::ChatCommand {
        dimension_epoch: dimension.epoch,
        sequence: request.sequence,
        text: command.to_owned(),
    }) {
        Ok(()) => {
            ui.mark_chat_send_enqueued(request.session, request.sequence);
        }
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {}
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            ui.push_local_chat_line(
                "Java connection closed before the command could be sent.".into(),
                now,
            );
            ui.confirm_chat_send(request.sequence);
        }
    }
    true
}
fn prepare(message: &str) -> Result<&str, &'static str> {
    let command=message.strip_prefix('/').ok_or("Java player chat signing and acknowledgements are not implemented; slash commands are available.")?;
    // Native ChatScreen length and SharedConstants.isAllowedChatCharacter, distinct
    // from the packet codec's larger well-framed string limit.
    if message.encode_utf16().count() > 256
        || message
            .chars()
            .any(|c| c < ' ' || c == '\u{7f}' || c == '§')
    {
        return Err("Java command contains unsupported chat input or exceeds256 UTF-16 units.");
    }
    Ok(command)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_input_never_invents_chat_signatures_or_rewrites_user_arguments() {
        assert_eq!(prepare("/time query daytime"), Ok("time query daytime"));
        assert_eq!(prepare("/lobby"), Ok("lobby"));
        assert!(prepare("hello").is_err());
        assert!(prepare("/help\nstop").is_err());
        assert!(prepare(&format!("/{}", "x".repeat(255))).is_ok());
        assert!(prepare(&format!("/{}", "x".repeat(256))).is_err());
    }
}
