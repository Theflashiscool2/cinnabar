//! Session-scoped Java profiles and player-list facts; entity IDs remain separate identities.
pub mod teams;
use super::hud::GameMode;
use crate::NbtValue;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct Property {
    pub name: Arc<str>,
    pub value: Arc<str>,
    pub signature: Option<Arc<str>>,
}
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: Arc<str>,
    pub properties: Arc<[Property]>,
    pub default_skin: DefaultSkin,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultSkin {
    pub texture: Arc<str>,
    pub slim: bool,
}
/// Server-supplied public material. Retention does not grant chat/signature trust.
#[derive(Clone, Debug)]
pub struct UnverifiedChatSession {
    pub id: [u8; 16],
    pub expires_at: i64,
    pub public_key: Arc<[u8]>,
    pub signature: Arc<[u8]>,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub uuid: [u8; 16],
    pub profile: Arc<Profile>,
    pub chat: Option<UnverifiedChatSession>,
    pub mode: GameMode,
    pub listed: bool,
    pub latency: i32,
    pub display_name: Option<NbtValue>,
    pub order: i32,
    pub show_hat: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Update {
    pub uuid: [u8; 16],
    pub profile: Option<Arc<Profile>>,
    pub chat: Option<Option<UnverifiedChatSession>>,
    pub mode: Option<GameMode>,
    pub listed: Option<bool>,
    pub latency: Option<i32>,
    pub display_name: Option<Option<NbtValue>>,
    pub order: Option<i32>,
    pub show_hat: Option<bool>,
}
#[derive(Debug, Default)]
pub struct Roster {
    session: u64,
    entries: BTreeMap<[u8; 16], Entry>,
    teams: teams::State,
    pub unknown_updates: u64,
}
impl Roster {
    pub fn new(session: u64) -> Self {
        Self {
            session,
            ..Default::default()
        }
    }
    pub fn update_team(&mut self, session: u64, update: teams::Update) -> bool {
        if session != self.session {
            return false;
        }
        self.teams.apply(update);
        true
    }
    pub fn team_parameters(&self, owner: &str) -> Option<&teams::Parameters> {
        self.teams.parameters(owner)
    }
    pub fn name_visible(&self, owner: &str, viewer: &str) -> bool {
        self.teams.visible(owner, viewer)
    }
    /// Native player invisibility is viewer-relative; team visibility remains a separate gate.
    pub fn player_name_visible(
        &self,
        owner: &str,
        viewer: &str,
        invisible: bool,
        viewer_spectator: bool,
    ) -> bool {
        self.name_visible(owner, viewer)
            && (!invisible || viewer_spectator || self.teams.sees_invisible(owner, viewer))
    }
    pub fn get(&self, uuid: &[u8; 16]) -> Option<&Entry> {
        self.entries.get(uuid)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }
    /// Matches native two-pass admission: all additions precede updates, and duplicate
    /// additions preserve the original profile. A tab-list removal never removes an entity.
    pub fn update(&mut self, session: u64, updates: Vec<Update>) -> bool {
        if session != self.session {
            return false;
        }
        for update in &updates {
            if let Some(profile) = &update.profile {
                self.entries.entry(update.uuid).or_insert_with(|| Entry {
                    uuid: update.uuid,
                    profile: Arc::clone(profile),
                    chat: None,
                    mode: GameMode::Survival,
                    listed: false,
                    latency: 0,
                    display_name: None,
                    order: 0,
                    show_hat: true,
                });
            }
        }
        for update in updates {
            let Some(entry) = self.entries.get_mut(&update.uuid) else {
                self.unknown_updates = self.unknown_updates.saturating_add(1);
                continue;
            };
            if let Some(value) = update.chat {
                entry.chat = value;
            }
            if let Some(value) = update.mode {
                entry.mode = value;
            }
            if let Some(value) = update.listed {
                entry.listed = value;
            }
            if let Some(value) = update.latency {
                entry.latency = value;
            }
            if let Some(value) = update.display_name {
                entry.display_name = value;
            }
            if let Some(value) = update.order {
                entry.order = value;
            }
            if let Some(value) = update.show_hat {
                entry.show_hat = value;
            }
        }
        true
    }
    pub fn remove(&mut self, session: u64, ids: &[[u8; 16]]) -> bool {
        if session != self.session {
            return false;
        }
        for id in ids {
            self.entries.remove(id);
        }
        true
    }
}

#[cfg(test)]
mod tests;

/// Native UUID.hashCode and floorMod selection over the version-owned default skin table.
/// No skin names or protocol-specific registry values are duplicated in the world owner.
pub fn default_skin_index(uuid: [u8; 16], count: usize) -> Option<usize> {
    let count = i32::try_from(count).ok().filter(|n| *n > 0)?;
    let hash = uuid
        .as_chunks::<4>()
        .0
        .iter()
        .fold(0u32, |hash, word| hash ^ u32::from_be_bytes(*word)) as i32;
    Some(hash.rem_euclid(count) as usize)
}

#[cfg(test)]
mod skin_index_tests {
    use super::*;
    #[test]
    fn signed_uuid_hashes_use_native_floor_mod() {
        // Independently invoked DefaultPlayerSkin.get(UUID) in InspectHeadProfile.
        for (word, expected) in [(0u32, 0usize), (0x80000000, 16), (u32::MAX, 17)] {
            let mut uuid = [0; 16];
            uuid[..4].copy_from_slice(&word.to_be_bytes());
            assert_eq!(default_skin_index(uuid, 18), Some(expected));
        }
        assert_eq!(default_skin_index([0; 16], 0), None);
        assert_eq!(default_skin_index([0; 16], usize::MAX), None);
    }
}
