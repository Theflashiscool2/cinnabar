//! Bounded, payload-free evidence for one native Java attempt. Never serializes an Envelope.
use java_protocol::{
    session::{Envelope, Event, entities, inventory},
    target,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
};

const RECENT: usize = 32;
const NOTICES: usize = 64;
const RETAINED_REPORTS: usize = 5;
const FILE: &str = "java-join-latest.json";

#[derive(Default)]
pub(super) struct Report {
    directory: Option<PathBuf>,
    completed: bool,
    events: u64,
    phases: Vec<&'static str>,
    recent: VecDeque<&'static str>,
    deferred: BTreeMap<&'static str, u64>,
    metadata: BTreeMap<&'static str, u64>,
    components: BTreeSet<&'static str>,
    notices: BTreeSet<&'static str>,
    discarded_notices: u64,
}
impl Report {
    pub fn directory(&mut self, directory: PathBuf) {
        self.directory = Some(directory);
    }
    pub fn notice(&mut self, notice: &'static str) -> bool {
        if self.notices.len() < NOTICES || self.notices.contains(notice) {
            self.notices.insert(notice)
        } else {
            self.discarded_notices = self.discarded_notices.saturating_add(1);
            false
        }
    }
    pub fn observe(&mut self, envelope: &Envelope) {
        self.events = self.events.saturating_add(1);
        let name = match &envelope.event {
            Event::Phase(phase) => {
                use java_protocol::session::Phase::*;
                let phase = match phase {
                    Connecting => "connecting",
                    Login => "login",
                    Configuration => "configuration",
                    Play => "play",
                };
                // Record which phases were reached, not an unbounded transition history.
                if !self.phases.contains(&phase) {
                    self.phases.push(phase);
                }
                phase
            }
            Event::DeferredPlay { name, .. } => {
                let name = target::game::clientbound::NAMES
                    .iter()
                    .copied()
                    .find(|n| *n == name)
                    .unwrap_or("unknown_deferred_packet");
                let n = self.deferred.entry(name).or_default();
                *n = n.saturating_add(1);
                name
            }
            Event::Entities {
                packet: entities::Packet::Metadata(packet),
                ..
            } => {
                if let Some(gap) = &packet.unsupported {
                    let name = target::METADATA_SERIALIZERS
                        .iter()
                        .copied()
                        .find(|n| *n == gap.serializer)
                        .unwrap_or("unknown_serializer");
                    let n = self.metadata.entry(name).or_default();
                    *n = n.saturating_add(1);
                }
                "entity_metadata"
            }
            Event::Inventory { packet, .. } => {
                match packet {
                    inventory::Packet::Content { slots, carried, .. } => {
                        for stack in slots.iter().chain(std::iter::once(carried)).flatten() {
                            self.stack(stack);
                        }
                    }
                    inventory::Packet::Slot { stack, .. }
                    | inventory::Packet::PlayerSlot { stack, .. }
                    | inventory::Packet::Cursor(stack) => {
                        if let Some(stack) = stack {
                            self.stack(stack);
                        }
                    }
                    _ => {}
                }
                "inventory"
            }
            Event::SelectionSent { .. } => "selection_sent",
            Event::BlockAck { .. } => "block_ack",
            Event::Roster(_) => "roster",
            Event::Hud(_) => "hud",
            Event::Bootstrap(_) => "bootstrap",
            Event::WorldLogin(_) => "world_login",
            Event::Tags(_) => "tags",
            Event::Teleport { .. } => "teleport",
            Event::Respawn { .. } => "respawn",
            Event::Rotation { .. } => "rotation",
            Event::Dynamics { .. } => "dynamics",
            Event::Entities { .. } => "entities",
            Event::KeepAlive(_) => "keepalive",
            Event::Vitals(_) => "vitals",
            Event::Death { .. } => "death",
            Event::Terrain { .. } => "terrain",
        };
        if self.recent.len() == RECENT {
            self.recent.pop_front();
        }
        self.recent.push_back(name);
    }
    fn stack(&mut self, stack: &inventory::Stack) {
        for name in stack.added.keys().chain(stack.removed.iter()) {
            if let Some(spec) = target::ITEM_COMPONENTS
                .iter()
                .find(|spec| spec.name == *name)
            {
                self.components.insert(spec.name);
            }
        }
    }
    fn document(&self, outcome: &str, error: Option<&str>) -> Value {
        // Error text may contain server-controlled chat, addresses or account information.
        // Export only categories and identities from the pinned, public protocol tables.
        let error = error.unwrap_or_default();
        let packet = target::game::clientbound::NAMES
            .iter()
            .chain(target::configuration::clientbound::NAMES.iter())
            .find(|name| error.contains(**name));
        let component = target::ITEM_COMPONENTS
            .iter()
            .find(|spec| error.contains(spec.name))
            .map(|s| s.name);
        let kind = if error.is_empty() {
            None
        } else {
            Some(if error.contains("could not be decoded") {
                "packet_decode"
            } else if error.contains("not implemented") || error.contains("unsupported") {
                "unsupported"
            } else if error.contains("authentication") || error.contains("account") {
                "authentication_or_account"
            } else if error.contains("I/O") || error.contains("timed out") {
                "network_or_timeout"
            } else {
                "connection_failure"
            })
        };
        json!({"format":1,"client_version":env!("CARGO_PKG_VERSION"),"java_version":target::GAME_VERSION,
            "protocol":target::PROTOCOL,"outcome":outcome,"terminal":{"category":kind,"packet":packet,"component":component},
            "error_details":"See the corresponding error in client.log; raw error text is omitted here for privacy.",
            "phases_reached":self.phases,"observed_events":self.events,"recent_event_types":self.recent,
            "deferred_packets":self.deferred,"unsupported_entity_serializers":self.metadata,
            "observed_item_components":self.components,"local_notices":self.notices,"discarded_notices":self.discarded_notices,
            "limits":"Observed events only. Deferred does not mean implemented. Fatal errors stop decoding; unseen packets cannot be diagnosed."})
    }
    /// Persist once on terminal result or retirement; no per-packet disk writes or console spam.
    pub fn finish(&mut self, outcome: &str, error: Option<&str>) {
        if self.completed {
            return;
        }
        self.completed = true;
        let Some(directory) = &self.directory else {
            return;
        };
        match write(directory, &self.document(outcome, error)) {
            Ok(()) => {
                tracing::info!(target:"java_session", report=%directory.join(FILE).display(), outcome,
                observed_events=self.events, deferred_types=self.deferred.len(), metadata_gaps=self.metadata.len(),
                "Java join diagnostic saved")
            }
            Err(error) => {
                tracing::warn!(target:"java_session", %error, "Could not save Java join diagnostic")
            }
        }
    }
}
fn write(directory: &Path, document: &Value) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(directory)?;
    let temporary = directory.join("java-join-writing.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(document)?)?;
    file.sync_all()?;
    drop(file);
    // Rotate only files owned by this reporter. Keep at most five completed attempts.
    for index in (1..RETAINED_REPORTS).rev() {
        let source = if index == 1 {
            directory.join(FILE)
        } else {
            directory.join(format!("java-join-{}.json", index - 1))
        };
        let destination = directory.join(format!("java-join-{index}.json"));
        if source.exists() {
            if destination.exists() {
                std::fs::remove_file(&destination)?;
            }
            std::fs::rename(source, destination)?;
        }
    }
    std::fs::rename(temporary, directory.join(FILE))
}
#[cfg(test)]
mod tests;
