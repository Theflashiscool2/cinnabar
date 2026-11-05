//! Connection-owned received chat and local command-write receipts, not server acknowledgements.
use std::collections::VecDeque;
use world::NbtValue;
/// Java26.3 ChatComponent retains the last100 full messages. Presentation may wrap them.
pub const HISTORY: usize = 100;
#[derive(Debug)]
pub struct Message {
    pub sequence: u64,
    pub content: NbtValue,
    pub overlay: bool,
    pub received_tick: u64,
}
#[derive(Default, Debug)]
pub struct State {
    pub messages: VecDeque<Message>,
    pub overlay: Option<Message>,
    /// The UI permits one write in flight, so this cannot overwrite an unconsumed receipt.
    pub written: Option<(u64, bool)>,
    pub suggestions: Option<Suggestions>,
    sequence: u64,
}
impl State {
    pub(super) fn receive(&mut self, message: java_protocol::session::chat::System, tick: u64) {
        self.sequence = self
            .sequence
            .checked_add(1)
            .expect("native chat sequence exhausted");
        let message = Message {
            sequence: self.sequence,
            content: super::nbt::convert(message.content),
            overlay: message.overlay,
            received_tick: tick,
        };
        if message.overlay {
            self.overlay = Some(message);
            return;
        }
        self.messages.push_back(message);
        if self.messages.len() > HISTORY {
            self.messages.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_history_is_bounded_and_overlay_does_not_displace_chat() {
        let mut state = State::default();
        for index in 0..105 {
            state.receive(
                java_protocol::session::chat::System {
                    content: java_protocol::nbt::Value::String(index.to_string()),
                    overlay: false,
                },
                index,
            );
        }
        assert_eq!(state.messages.len(), 100);
        assert_eq!(state.messages.front().unwrap().sequence, 6);
        for index in 105..220 {
            state.receive(
                java_protocol::session::chat::System {
                    content: java_protocol::nbt::Value::String("overlay".into()),
                    overlay: true,
                },
                index,
            );
        }
        assert_eq!(state.messages.len(), 100);
        assert_eq!(state.messages.front().unwrap().sequence, 6);
        assert_eq!(state.overlay.unwrap().received_tick, 219);
    }
}

/// One latest server completion response; the editor correlates it with its active input.
#[derive(Debug)]
pub struct Suggestions {
    pub id: i32,
    pub start: i32,
    pub length: i32,
    pub entries: Vec<(String, Option<NbtValue>)>,
}
impl From<java_protocol::session::chat::suggestions::Packet> for Suggestions {
    fn from(packet: java_protocol::session::chat::suggestions::Packet) -> Self {
        Self {
            id: packet.id,
            start: packet.start,
            length: packet.length,
            entries: packet
                .entries
                .into_iter()
                .map(|e| (e.text, e.tooltip.map(super::nbt::convert)))
                .collect(),
        }
    }
}
