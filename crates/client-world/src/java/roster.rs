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
        wire::Packet::Team(packet) => {
            use domain::teams as d;
            use wire::teams::Visibility as W;
            let action = match packet.method {
                0 => d::Action::Create,
                1 => d::Action::Remove,
                2 => d::Action::Update,
                3 => d::Action::Join,
                4 => d::Action::Leave,
                _ => d::Action::Unknown,
            };
            state.players.update_team(
                state.generation,
                d::Update {
                    team: packet.team,
                    action,
                    members: packet.members,
                    parameters: packet.parameters.map(|p| d::Parameters {
                        display: super::nbt::convert(p.display),
                        prefix: super::nbt::convert(p.prefix),
                        suffix: super::nbt::convert(p.suffix),
                        visibility: match p.visibility {
                            W::Always => d::Visibility::Always,
                            W::Never => d::Visibility::Never,
                            W::OtherTeamsHidden => d::Visibility::OtherTeamsHidden,
                            W::OwnTeamHidden => d::Visibility::OwnTeamHidden,
                        },
                        collision: match p.collision {
                            1 => d::Collision::Never,
                            2 => d::Collision::OtherTeams,
                            3 => d::Collision::OwnTeam,
                            _ => d::Collision::Always,
                        },
                        color: p.color,
                        options: p.options,
                    }),
                },
            )
        }
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
    let skins = java_protocol::target::DEFAULT_PLAYER_SKINS;
    let skin =
        &skins[domain::default_skin_index(uuid, skins.len()).expect("verified native skin table")];
    domain::DefaultSkin {
        texture: skin.texture.into(),
        slim: skin.slim,
    }
}

#[cfg(test)]
mod tests;
