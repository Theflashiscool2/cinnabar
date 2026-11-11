//! Decoded player capability authority and local write receipts, never server acceptance.
use super::{Connection, Error, Event, dynamics, hud};
use crate::ProtocolError;

#[derive(Clone, Debug, serde::Serialize)]
pub enum Update {
    Values(dynamics::Abilities),
    Mode(hud::GameMode),
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Written {
    pub dimension_epoch: u64,
    pub authority_revision: u64,
    pub flying: bool,
    pub written: bool,
}

impl Connection {
    pub(super) async fn ability_update(&mut self, update: Update) -> Result<(), Error> {
        self.ability_revision = self
            .ability_revision
            .checked_add(1)
            .ok_or(ProtocolError::Invalid("Java ability authority overflow"))?;
        self.emit(Event::Abilities {
            dimension_epoch: self.dimension_epoch,
            authority_revision: self.ability_revision,
            update,
        })
        .await
    }
}

impl Connection {
    /// Native PacketProcessor drains already-ready tasks before aiStep writes a capability
    /// change. Poll only ready frames: partial/cipher progress remains in cancellation-safe
    /// Stream::read, and Pending immediately permits the held command without a time delay.
    /// Continuously ready ingress can postpone the head ability and its FIFO siblings, as
    /// native ready-task draining can. No promise covers a future, not-yet-ready packet.
    pub(super) async fn ready_ability_packet(
        &mut self,
    ) -> Result<Option<(super::ingress::Frame, bool)>, Error> {
        if let Some(frame) = self.chat_pending.pop() {
            return Ok(Some((frame, true)));
        }
        if let Some(frame) = self.ingress.pop() {
            return Ok(Some((frame, false)));
        }
        let ready = self.stream.read_ready().await;
        ready
            .map(|result| {
                result
                    .map(|bytes| (super::ingress::Frame::new(bytes), false))
                    .map_err(Error::from)
            })
            .transpose()
    }
}
