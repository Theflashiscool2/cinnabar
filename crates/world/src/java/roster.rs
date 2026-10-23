//! Session-scoped Java profiles and player-list facts; entity IDs remain separate identities.
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
    pub unknown_updates: u64,
}
impl Roster {
    pub fn new(session: u64) -> Self {
        Self {
            session,
            ..Default::default()
        }
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
