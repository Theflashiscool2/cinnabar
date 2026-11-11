//! Ordered native session messages exposed to admission consumers.
use super::{Phase, border, chat, dynamics, entities, facts, hud, inventory, roster, scoreboard};
use facts::Bootstrap;

#[derive(Clone, Debug, serde::Serialize)]
pub enum Event {
    Abilities {
        dimension_epoch: u64,
        authority_revision: u64,
        update: super::abilities::Update,
    },
    AbilitiesWritten(super::abilities::Written),
    Border {
        dimension_epoch: u64,
        packet: border::Packet,
    },
    CommandTree {
        nodes: usize,
        message_arguments: usize,
    },
    CommandPermission(u8),
    CommandSuggestions(chat::suggestions::Packet),
    SystemChat(chat::System),
    /// Local socket-write receipt, never a server command-success acknowledgement.
    ChatCommandSent {
        sequence: u64,
        written: bool,
    },
    /// Local write receipt, not a server transaction acknowledgement.
    SelectionSent {
        dimension_epoch: u64,
        sequence: u64,
        slot: u8,
    },
    /// Local serialization receipt, never a server acceptance acknowledgement.
    InventorySent {
        dimension_epoch: u64,
        incarnation: u64,
        sequence: u64,
        container: i32,
        closed: bool,
        written: bool,
    },
    /// Local socket-write receipt. The display may predict; storage remains server-owned.
    DropWritten {
        dimension_epoch: u64,
        sequence: u64,
        wire_revision: u64,
        slot: u8,
        whole_stack: bool,
        written: bool,
    },
    Inventory {
        dimension_epoch: u64,
        wire_revision: u64,
        incarnation: u64,
        packet: inventory::Packet,
    },
    BlockAck {
        dimension_epoch: u64,
        sequence: i32,
    },
    Roster(roster::Packet),
    Hud(hud::Packet),
    Scoreboard(scoreboard::Packet),
    Phase(Phase),
    /// Validated server-directed destination; cookies and credentials never leave the task.
    ServerTransfer {
        address: String,
    },
    Bootstrap(Box<Bootstrap>),
    /// A further play login replaces the level while retaining this connection's registries.
    WorldLogin(Box<facts::World>),
    /// Session-scoped registry tag replacements, ordered with other play authority.
    Tags(facts::Tags),
    Teleport {
        dimension_epoch: u64,
        sequence: u64,
        correction: facts::Teleport,
    },
    Respawn {
        spawn: facts::Spawn,
        keep_attribute_modifiers: bool,
        keep_metadata: bool,
    },
    Rotation {
        dimension_epoch: u64,
        sequence: u64,
        yaw: f32,
        relative_yaw: bool,
        pitch: f32,
        relative_pitch: bool,
    },
    Dynamics {
        dimension_epoch: u64,
        packet: dynamics::Packet,
    },
    Entities {
        dimension_epoch: u64,
        packet: entities::Packet,
    },
    /// Game-thread ordering barrier, distinct from a network-thread keep-alive.
    Ping {
        sequence: u64,
        id: i32,
    },
    KeepAlive(i64),
    Vitals(facts::Vitals),
    Death {
        player_id: i32,
        message: crate::nbt::Value,
    },
    Terrain {
        dimension_epoch: u64,
        packet: Box<crate::terrain::Packet>,
    },
    /// Known wire packets not yet admitted to gameplay; diagnostics retain the distinction.
    DeferredPlay {
        name: String,
        bytes: Vec<u8>,
    },
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Envelope {
    pub generation: u64,
    pub event: Event,
}
