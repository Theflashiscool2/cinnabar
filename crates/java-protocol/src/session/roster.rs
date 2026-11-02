//! Native player-list wire schemas, separate from entity IDs and local authentication.
pub mod teams;
use super::hud::GameMode;
use crate::{ProtocolError as Error, nbt, target, wire::Reader};
use rsa::pkcs8::DecodePublicKey;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Profile {
    pub name: String,
    pub properties: Vec<Property>,
}
/// Received public material, not authenticated chat authority. A chat consumer must verify it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ChatSession {
    pub id: [u8; 16],
    pub expires_at: i64,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Update {
    pub uuid: [u8; 16],
    pub profile: Option<Profile>,
    pub chat: Option<Option<ChatSession>>,
    pub mode: Option<GameMode>,
    pub listed: Option<bool>,
    pub latency: Option<i32>,
    pub display_name: Option<Option<nbt::Value>>,
    pub order: Option<i32>,
    pub show_hat: Option<bool>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Packet {
    Team(teams::Packet),
    Update(Vec<Update>),
    Remove(Vec<[u8; 16]>),
}

/// Uses the pinned action order; optional values distinguish no update from explicit removal.
pub fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, Error> {
    Ok(Some(match name {
        "ClientboundSetPlayerTeamPacket" => Packet::Team(teams::decode(r)?),
        "ClientboundPlayerInfoRemovePacket" => {
            let count = r.count(r.0.len() / 16)?;
            Packet::Remove(
                (0..count)
                    .map(|_| r.uuid().map(|id| id.into_bytes()))
                    .collect::<Result<_, _>>()?,
            )
        }
        "ClientboundPlayerInfoUpdatePacket" => {
            let mask = r
                .take(target::PLAYER_LIST_ACTION_NAMES.len().div_ceil(8))?
                .to_vec();
            let count = r.count(r.0.len() / 16)?;
            let mut updates = Vec::with_capacity(count);
            for _ in 0..count {
                let mut update = Update {
                    uuid: r.uuid()?.into_bytes(),
                    ..Default::default()
                };
                for (bit, action) in target::PLAYER_LIST_ACTION_NAMES.iter().enumerate() {
                    if mask[bit / 8] & (1 << (bit % 8)) == 0 {
                        continue;
                    }
                    match *action {
                        "ADD_PLAYER" => {
                            let name = r.string(16)?;
                            let count = r.count(r.0.len())?;
                            let properties = (0..count)
                                .map(|_| {
                                    Ok(Property {
                                        name: r.string(32767)?,
                                        value: r.string(32767)?,
                                        signature: if r.bool()? {
                                            Some(r.string(32767)?)
                                        } else {
                                            None
                                        },
                                    })
                                })
                                .collect::<Result<_, Error>>()?;
                            update.profile = Some(Profile { name, properties });
                        }
                        "INITIALIZE_CHAT" => {
                            update.chat = Some(if r.bool()? {
                                Some(chat_session(r)?)
                            } else {
                                None
                            })
                        }
                        "UPDATE_GAME_MODE" => update.mode = Some(GameMode::from_id(r.var()?)),
                        "UPDATE_LISTED" => update.listed = Some(r.bool()?),
                        "UPDATE_LATENCY" => update.latency = Some(r.var()?),
                        "UPDATE_DISPLAY_NAME" => {
                            update.display_name =
                                Some(if r.bool()? { Some(nbt::read(r)?) } else { None })
                        }
                        "UPDATE_LIST_ORDER" => update.order = Some(r.var()?),
                        "UPDATE_HAT" => update.show_hat = Some(r.bool()?),
                        _ => return Err(Error::Invalid("unimplemented Java player-list action")),
                    }
                }
                updates.push(update);
            }
            Packet::Update(updates)
        }
        _ => return Ok(None),
    }))
}

fn chat_session(r: &mut Reader<'_>) -> Result<ChatSession, Error> {
    let id = r.uuid()?.into_bytes();
    let expires_at = r.i64()?;
    let public_key = r.bytes(r.0.len())?;
    rsa::RsaPublicKey::from_public_key_der(&public_key)
        .map_err(|_| Error::Invalid("Java remote chat public key encoding"))?;
    let signature = r.bytes(r.0.len())?;
    Ok(ChatSession {
        id,
        expires_at,
        public_key,
        signature,
    })
}

#[cfg(test)]
mod tests;
