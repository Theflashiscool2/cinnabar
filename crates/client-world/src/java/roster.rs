//! Admits native profiles without exposing packet structures to entity/render owners.
use super::WorldState;
use java_protocol::session::roster as wire;
use std::sync::Arc;
use world::java::roster as domain;

pub(super) fn admit(state: &mut WorldState, packet: wire::Packet) -> bool {
    if state.bootstrap.is_none() {
        return false;
    }
    match packet {
        wire::Packet::Remove(ids) => state.players.remove(state.generation, &ids),
        wire::Packet::Update(updates) => state.players.update(
            state.generation,
            updates
                .into_iter()
                .map(|u| domain::Update {
                    uuid: u.uuid,
                    profile: u.profile.map(|p| {
                        Arc::new(domain::Profile {
                            name: p.name.into(),
                            default_skin: default_skin(u.uuid),
                            properties: p
                                .properties
                                .into_iter()
                                .map(|p| domain::Property {
                                    name: p.name.into(),
                                    value: p.value.into(),
                                    signature: p.signature.map(Into::into),
                                })
                                .collect(),
                        })
                    }),
                    chat: u.chat.map(|value| {
                        value.map(|c| domain::UnverifiedChatSession {
                            id: c.id,
                            expires_at: c.expires_at,
                            public_key: c.public_key.into(),
                            signature: c.signature.into(),
                        })
                    }),
                    mode: u.mode.map(super::hud::mode),
                    listed: u.listed,
                    latency: u.latency,
                    display_name: u.display_name.map(|n| n.map(super::nbt::convert)),
                    order: u.order,
                    show_hat: u.show_hat,
                })
                .collect(),
        ),
    }
}

/// Java UUID.hashCode and floorMod choose the runtime-exported default skin table.
fn default_skin(uuid: [u8; 16]) -> domain::DefaultSkin {
    let hash = uuid
        .as_chunks::<4>()
        .0
        .iter()
        .fold(0u32, |hash, word| hash ^ u32::from_be_bytes(*word)) as i32;
    let skins = java_protocol::target::DEFAULT_PLAYER_SKINS;
    let skin = &skins[hash.rem_euclid(skins.len() as i32) as usize];
    domain::DefaultSkin {
        texture: skin.texture.into(),
        slim: skin.slim,
    }
}

#[cfg(test)]
mod tests;
