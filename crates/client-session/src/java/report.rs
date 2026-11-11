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
    telemetry: Option<std::sync::Arc<super::telemetry::Recorder>>,
    completed: bool,
    menus: BTreeMap<&'static str, u64>,
    menu_slots: BTreeMap<&'static str, [usize; 2]>,
    active_menu: Option<(u64, u64, i32, &'static str)>,
    entity_spawns: BTreeMap<&'static str, u64>,
    world_logins: u64,
    respawns: u64,
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
    pub(super) fn with_telemetry(
        telemetry: Option<std::sync::Arc<super::telemetry::Recorder>>,
    ) -> Self {
        Self {
            telemetry,
            ..Self::default()
        }
    }
    pub(super) fn set_telemetry(
        &mut self,
        telemetry: Option<std::sync::Arc<super::telemetry::Recorder>>,
    ) {
        self.telemetry = telemetry;
    }
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
            Event::ServerTransfer { .. } => "server_transfer",
            Event::Phase(phase) => {
                use java_protocol::session::Phase::*;
                let phase = match phase {
                    Connecting => "connecting",
                    Login => "login",
                    Configuration => "configuration",
                    Play => "play",
                };
                if matches!(phase, "configuration") {
                    self.active_menu = None;
                }
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
            Event::Entities {
                packet: entities::Packet::Spawn(entity),
                ..
            } => {
                let kind = target::ENTITY_TYPES
                    .iter()
                    .copied()
                    .find(|kind| *kind == entity.kind)
                    .unwrap_or("unknown_entity");
                let count = self.entity_spawns.entry(kind).or_default();
                *count = count.saturating_add(1);
                "entity_spawn"
            }
            Event::Inventory {
                packet,
                dimension_epoch,
                incarnation,
                ..
            } => {
                match packet {
                    inventory::Packet::Open {
                        container, kind, ..
                    } => {
                        let kind = target::CONTAINER_TYPES
                            .iter()
                            .copied()
                            .find(|k| k == kind)
                            .unwrap_or("unknown_menu");
                        let n = self.menus.entry(kind).or_default();
                        *n = n.saturating_add(1);
                        self.active_menu = Some((*dimension_epoch, *incarnation, *container, kind));
                    }
                    inventory::Packet::Close(id)
                        if self.active_menu.is_some_and(|m| m.2 == *id) =>
                    {
                        self.active_menu = None
                    }
                    _ => {}
                }
                if let inventory::Packet::Content {
                    container, slots, ..
                } = packet
                    && let Some((epoch, opening, id, kind)) = self.active_menu
                    && (epoch, opening, id) == (*dimension_epoch, *incarnation, *container)
                {
                    let range = self.menu_slots.entry(kind).or_insert([slots.len(); 2]);
                    range[0] = range[0].min(slots.len());
                    range[1] = range[1].max(slots.len());
                }
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
            Event::CommandTree { .. } => "command_tree",
            Event::CommandPermission(_) => "command_permission",
            Event::CommandSuggestions(_) => "command_suggestions",
            Event::SystemChat(_) => "system_chat",
            Event::ChatCommandSent { written: true, .. } => "command_write_receipt",
            Event::ChatCommandSent { written: false, .. } => "command_input_retired",
            Event::SelectionSent { .. } => "selection_sent",
            Event::DropWritten { written: true, .. } => "drop_write_receipt",
            Event::DropWritten { written: false, .. } => "drop_input_retired",
            Event::InventorySent {
                dimension_epoch,
                incarnation,
                container,
                closed: true,
                written: true,
                ..
            } => {
                if self.active_menu.is_some_and(|m| {
                    (m.0, m.1, m.2) == (*dimension_epoch, *incarnation, *container)
                }) {
                    self.active_menu = None;
                }
                "inventory_write_receipt"
            }
            Event::InventorySent { written: true, .. } => "inventory_write_receipt",
            Event::InventorySent { written: false, .. } => "inventory_input_retired",
            Event::BlockAck { .. } => "block_ack",
            Event::Roster(_) => "roster",
            Event::Hud(_) => "hud",
            Event::Abilities { .. } => "ability_authority",
            Event::AbilitiesWritten(receipt) => {
                if receipt.written {
                    "ability_written"
                } else {
                    "ability_not_written"
                }
            }
            Event::Bootstrap(_) => {
                self.active_menu = None;
                self.world_logins = self.world_logins.saturating_add(1);
                "bootstrap"
            }
            Event::WorldLogin(_) => {
                self.active_menu = None;
                self.world_logins = self.world_logins.saturating_add(1);
                "world_login"
            }
            Event::Scoreboard(_) => "scoreboard",
            Event::Tags(_) => "tags",
            Event::Teleport { .. } => "teleport",
            Event::Respawn { .. } => {
                self.active_menu = None;
                self.respawns = self.respawns.saturating_add(1);
                "respawn"
            }
            Event::Rotation { .. } => "rotation",
            Event::Dynamics { .. } => "dynamics",
            Event::Entities { .. } => "entities",
            Event::Ping { .. } => "ping",
            Event::KeepAlive(_) => "keepalive",
            Event::Vitals(_) => "vitals",
            Event::Death { .. } => "death",
            Event::Terrain { .. } => "terrain",
            Event::Border { .. } => "border",
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
            "protocol":target::PROTOCOL,"source_commit":option_env!("CINNABAR_BUILD_COMMIT"),"outcome":outcome,"terminal":{"category":kind,"packet":packet,"component":component},
            "error_details":"See the corresponding error in client.log; raw error text is omitted here for privacy.",
            "phases_reached":self.phases,"observed_events":self.events,"recent_event_types":self.recent,
            "server_menu_openings":self.menus,"server_menu_slot_ranges":self.menu_slots,
            "entity_spawn_types":self.entity_spawns,"world_logins":self.world_logins,"respawns":self.respawns,
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
        let telemetry = self.telemetry.as_ref().map(|t| t.finish());
        let Some(directory) = &self.directory else {
            return;
        };
        let mut document = self.document(outcome, error);
        if let Some(telemetry) = telemetry {
            document["telemetry"] = telemetry;
        }
        match write(directory, &document) {
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
