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

/// Match the normal play reader's admissibility predicates. Chat trust defers only its
/// chat queue, whereas a correction/ping defers game ingress. Fresh frames still pass the
/// ordinary play classifier; this function never admits a buffered correction early.
/// Continuously ready input can postpone the head ability and its FIFO siblings.
pub(super) async fn ready_packet<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut crate::stream::Stream<S>,
    ingress: &mut super::ingress::Ingress,
    chat_pending: &mut super::ingress::Ingress,
    awaiting_authority: bool,
    preparing_chat: bool,
) -> Result<Option<(super::ingress::Frame, bool)>, Error> {
    // Unlike a packet/time cap, Tokio's checkpoint only yields when the task has spent
    // its cooperative budget. Await the yield before polling: it is not an empty socket.
    tokio::task::consume_budget().await;
    if (awaiting_authority && !ingress.can_read()) || (preparing_chat && !chat_pending.can_read()) {
        return Ok(None);
    }
    if !awaiting_authority
        && !preparing_chat
        && let Some(frame) = chat_pending.pop()
    {
        return Ok(Some((frame, true)));
    }
    if !awaiting_authority && let Some(frame) = ingress.pop() {
        return Ok(Some((frame, false)));
    }
    // Poll once without waiting. Stream owns partial framing/cipher progress and does not
    // mistake cooperative budget exhaustion for absent bytes. Future bytes are not delayed for.
    stream
        .read_ready()
        .await
        .map(|result| {
            result
                .map(|bytes| (super::ingress::Frame::new(bytes), false))
                .map_err(Error::from)
        })
        .transpose()
}

#[cfg(test)]
mod ready_tests {
    use super::super::ingress::{Frame, Ingress};
    use super::*;
    use crate::{codec, stream::Stream};

    fn packet(name: &str, payload: &[u8]) -> Vec<u8> {
        let id = crate::target::game::clientbound::NAMES
            .iter()
            .position(|n| *n == name)
            .unwrap();
        let mut bytes = Vec::new();
        codec::write_varint(id as i32, &mut bytes);
        bytes.extend(payload);
        bytes
    }

    #[tokio::test]
    async fn pending_chat_trust_keeps_deferred_chat_but_reads_capability_authority() {
        let (io, peer) = tokio::io::duplex(1024);
        let (mut stream, mut server) = (Stream::new(io), Stream::new(peer));
        let (mut ingress, mut chat) = (Ingress::new(4096), Ingress::new(4096));
        // Chat has not been signature-decoded yet: it must remain untouched until roots arrive.
        let deferred = packet("ClientboundPlayerChatPacket", &[]);
        chat.push(Frame::new(deferred.clone()));
        let mode = packet("ClientboundGameEventPacket", &[3, 64, 64, 0, 0]);
        ingress.push(Frame::new(mode.clone()));
        let (frame, replayed_chat) =
            ready_packet(&mut stream, &mut ingress, &mut chat, false, true)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(frame.bytes, mode);
        assert!(!replayed_chat);
        let values = packet(
            "ClientboundPlayerAbilitiesPacket",
            &[7, 61, 76, 204, 205, 61, 204, 204, 205],
        );
        server.write(&values).await.unwrap();
        let (frame, replayed_chat) =
            ready_packet(&mut stream, &mut ingress, &mut chat, false, true)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(frame.bytes, values);
        assert!(!replayed_chat);
        assert!(
            ready_packet(&mut stream, &mut ingress, &mut chat, false, true)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(chat.pop().unwrap().bytes, deferred);
    }

    #[tokio::test]
    async fn correction_and_ping_preserve_buffered_world_order_while_reading_fresh_frames() {
        let (io, peer) = tokio::io::duplex(1024);
        let (mut stream, mut server) = (Stream::new(io), Stream::new(peer));
        let (mut ingress, mut chat) = (Ingress::new(4096), Ingress::new(4096));
        let queued = packet("ClientboundGameEventPacket", &[3, 64, 64, 0, 0]);
        ingress.push(Frame::new(queued.clone()));
        let fresh = packet("ClientboundKeepAlivePacket", &123_i64.to_be_bytes());
        server.write(&fresh).await.unwrap();
        let (frame, replayed_chat) =
            ready_packet(&mut stream, &mut ingress, &mut chat, true, false)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(frame.bytes, fresh);
        assert!(!replayed_chat);
        assert_eq!(ingress.pop().unwrap().bytes, queued);
    }

    #[tokio::test]
    async fn pending_chat_budget_and_blocked_codec_boundary_stop_fresh_reads() {
        for chat_blocked in [false, true] {
            let (io, peer) = tokio::io::duplex(1024);
            let (mut stream, mut server) = (Stream::new(io), Stream::new(peer));
            let (mut ingress, mut chat) = (Ingress::new(4096), Ingress::new(1));
            if chat_blocked {
                chat.push(Frame::new(vec![0]));
            } else {
                let mut boundary = Frame::new(packet("ClientboundStartConfigurationPacket", &[]));
                boundary.boundary = true;
                ingress.push(boundary);
            }
            let values = packet("ClientboundGameEventPacket", &[3, 64, 64, 0, 0]);
            server.write(&values).await.unwrap();
            assert!(
                ready_packet(
                    &mut stream,
                    &mut ingress,
                    &mut chat,
                    !chat_blocked,
                    chat_blocked
                )
                .await
                .unwrap()
                .is_none()
            );
            assert_eq!(
                stream.read_ready().await.unwrap().unwrap(),
                values,
                "blocked reader must not consume bytes from the later codec or trust context"
            );
        }
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::super::ingress::{Frame, Ingress};
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn finite_ready_ingress_yields_to_cancellation_before_the_queue_is_empty() {
        const FRAMES: usize = 4096;
        let (io, peer) = tokio::io::duplex(16);
        let mut stream = crate::stream::Stream::new(io);
        let (mut ingress, mut chat) = (Ingress::new(1_000_000), Ingress::new(4096));
        for _ in 0..FRAMES {
            ingress.push(Frame::new(vec![0]));
        }
        let processed = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&processed);
        let cancel = tokio_util::sync::CancellationToken::new();
        let child = cancel.clone();
        let (started, running) = tokio::sync::oneshot::channel();
        let worker = tokio::spawn(async move {
            let _peer = peer;
            let mut started = Some(started);
            let drain = async move {
                for _ in 0..FRAMES {
                    assert!(
                        ready_packet(&mut stream, &mut ingress, &mut chat, false, false)
                            .await
                            .unwrap()
                            .is_some()
                    );
                    count.fetch_add(1, Ordering::Relaxed);
                    if let Some(started) = started.take() {
                        started.send(()).unwrap();
                    }
                }
            };
            // Same cancellation boundary as run(): an always-ready inner poll must yield.
            tokio::select! { biased; _ = child.cancelled() => true, _ = drain => false }
        });
        running.await.unwrap();
        cancel.cancel();
        assert!(worker.await.unwrap());
        assert!((1..FRAMES).contains(&processed.load(Ordering::Relaxed)));
    }
}
