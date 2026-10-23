//! Ordered native session messages exposed to admission consumers.
use super::{Phase, dynamics, entities, facts, hud, roster};
use facts::Bootstrap;

#[derive(Clone, Debug, serde::Serialize)]
pub enum Event {
    BlockAck {
        dimension_epoch: u64,
        sequence: i32,
    },
    Roster(roster::Packet),
    Hud(hud::Packet),
    Phase(Phase),
    Bootstrap(Box<Bootstrap>),
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
