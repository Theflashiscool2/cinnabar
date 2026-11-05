//! Connection-local signing and validated remote-session ownership.
use super::{
    player::Player,
    security::{Body, Roots},
    window::Signature,
};
use crate::{ProtocolError as Error, auth::chat::Certificate, session::roster, wire};
use rsa::{Pkcs1v15Sign, RsaPublicKey, rand_core::RngCore};
use sha2::Digest;
use std::collections::BTreeMap;
/// Native ProfilePublicKey.EXPIRY_GRACE_PERIOD, verified against26.3 bytecode.
const REMOTE_GRACE_MILLIS: i64 = 8 * 60 * 60 * 1000;
pub(super) struct Local {
    pub certificate: std::sync::Arc<Certificate>,
    id: [u8; 16],
    index: i32,
}
impl Local {
    pub fn new(certificate: std::sync::Arc<Certificate>) -> Self {
        let mut bytes = [0; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self {
            certificate,
            id: uuid::Builder::from_random_bytes(bytes)
                .into_uuid()
                .into_bytes(),
            index: 0,
        }
    }
    pub fn announcement(&self) -> Result<Vec<u8>, Error> {
        let mut out = self.id.to_vec();
        out.extend(self.certificate.expires_at.to_be_bytes());
        wire::bytes(&self.certificate.public_der, &mut out)?;
        wire::bytes(&self.certificate.signature, &mut out)?;
        Ok(out)
    }
    pub fn sign(
        &mut self,
        text: &str,
        timestamp: i64,
        salt: i64,
        last_seen: &[Signature],
    ) -> Result<Signature, Error> {
        if self.certificate.expires_at < timestamp {
            return Err(Error::Invalid("local Java chat certificate expired"));
        }
        let body = Body {
            sender: self.certificate.owner,
            session: self.id,
            index: self.index,
            text,
            timestamp_millis: timestamp,
            salt,
            last_seen,
        };
        let bytes = self
            .certificate
            .private
            .sign_with_rng(
                &mut rand::rngs::OsRng,
                Pkcs1v15Sign::new::<sha2::Sha256>(),
                &sha2::Sha256::digest(body.payload()?),
            )
            .map_err(|_| Error::Invalid("Java message signing failed"))?;
        let signature = bytes
            .try_into()
            .map_err(|_| Error::Invalid("Java signature length"))?;
        self.index = self
            .index
            .checked_add(1)
            .ok_or(Error::Invalid("Java local chat index exhausted"))?;
        Ok(signature)
    }
}
struct Remote {
    data: roster::ChatSession,
    key: Option<RsaPublicKey>,
    last: Option<Player>,
    valid: bool,
}
#[derive(Default)]
pub(super) struct Remotes(BTreeMap<[u8; 16], Remote>);
impl Remotes {
    pub fn observe(&mut self, packet: &roster::Packet, roots: Option<&Roots>) {
        match packet {
            roster::Packet::Remove(ids) => {
                for id in ids {
                    self.0.remove(id);
                }
            }
            roster::Packet::Update(updates) => {
                for update in updates {
                    if let Some(chat) = &update.chat {
                        match chat {
                            None => {
                                self.0.remove(&update.uuid);
                            }
                            Some(data) => {
                                let key = roots.and_then(|roots| {
                                    roots
                                        .verify_profile(
                                            update.uuid,
                                            data.expires_at,
                                            &data.public_key,
                                            &data.signature,
                                        )
                                        .ok()
                                });
                                self.0.insert(
                                    update.uuid,
                                    Remote {
                                        data: data.clone(),
                                        key,
                                        last: None,
                                        valid: true,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            roster::Packet::Team(_) => {}
        }
    }
    pub fn install_roots(&mut self, roots: &Roots) {
        for (uuid, remote) in &mut self.0 {
            remote.key = roots
                .verify_profile(
                    *uuid,
                    remote.data.expires_at,
                    &remote.data.public_key,
                    &remote.data.signature,
                )
                .ok();
        }
    }
    /// A received certificate alone never grants identity. A broken signed chain stays broken
    /// until an authoritative new chat-session update, matching SignedMessageValidator.KeyBased.
    pub fn verify(&mut self, message: &Player, now: i64, enforced: bool) -> bool {
        let Some(remote) = self.0.get_mut(&message.sender) else {
            return message.signature.is_none() && !enforced;
        };
        let Some(key) = &remote.key else { return false };
        let Some(signature) = &message.signature else {
            remote.valid = false;
            return false;
        };
        let Some(last) = &remote.last else {
            if !remote.valid {
                return false;
            }
            let valid = remote.data.expires_at.saturating_add(REMOTE_GRACE_MILLIS) >= now
                && body(message, remote.data.id).verify(key, signature).is_ok();
            remote.valid = valid;
            if valid {
                remote.last = Some(message.clone());
            }
            return valid;
        };
        let chain = message.index > last.index
            || (message.index == last.index && message.same_message(last));
        remote.valid = remote.valid
            && chain
            && remote.data.expires_at.saturating_add(REMOTE_GRACE_MILLIS) >= now
            && body(message, remote.data.id).verify(key, signature).is_ok();
        if remote.valid {
            remote.last = Some(message.clone());
        }
        remote.valid
    }
}
fn body(message: &Player, session: [u8; 16]) -> Body<'_> {
    Body {
        sender: message.sender,
        session,
        index: message.index,
        text: &message.text,
        timestamp_millis: message.timestamp,
        salt: message.salt,
        last_seen: &message.last_seen,
    }
}
