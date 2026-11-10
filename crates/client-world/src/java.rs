//! Java admission boundary. Initial authoritative facts stay distinct from Bedrock world state.
mod biomes;
pub mod border;
pub mod chat;
mod dynamics;
mod entities;
mod environment;
pub mod equipment;
mod hud;
#[cfg(test)]
mod interaction_tests;
mod inventory;
pub mod lighting;
mod nbt;
pub mod piston;
pub mod player;
#[cfg(test)]
mod reconfiguration_tests;
mod roster;
pub mod scoreboard;
#[cfg(test)]
mod tags_tests;
mod terrain;
#[cfg(test)]
mod tests_support;
mod world_login;
use java_protocol::session::{
    Envelope, Event, Phase,
    facts::{Bootstrap, Spawn},
};
#[derive(Default, Debug)]
pub struct WorldState {
    pub border: border::Authority,
    pub chat: chat::State,
    pub inventory: ::inventory::java::State,
    generation: u64,
    dimension_epoch: u64,
    pub terrain: world::java::Terrain,
    pub lighting: lighting::Engine,
    piston_registry: Option<std::sync::Arc<assets::registry::java::BlockRegistry>>,
    pub entities: world::java::entities::Entities,
    pub equipment: equipment::State,
    pub players: world::java::roster::Roster,
    /// A live player entity retains its PlayerInfo after tab-list removal.
    removed_player_modes:
        std::collections::BTreeMap<world::java::entities::Identity, world::java::hud::GameMode>,
    pub player_visual: world::java::entities::visual::Visual,
    pub scoreboard: scoreboard::State,
    pub status: world::java::hud::State,
    pub biomes: Vec<world::java::BiomeAppearance>,
    pub terrain_error: Option<&'static str>,
    pub decoded_chunks: u64,
    pub block_updates: u64,
    /// Processing receipt only; acceptance comes from ordered authoritative block updates.
    pub block_ack: Option<(u64, i32)>,
    /// Ordered ACK reconciliation candidates for the existing native movement owner.
    pub block_reconciliations: std::collections::VecDeque<world::java::Reconciliation>,
    pub block_predictions_confirmed: u64,
    pub block_predictions_reverted: u64,
    pub block_predictions_retired: u64,
    pub vitals: Option<java_protocol::session::facts::Vitals>,
    pub death: Option<(i32, java_protocol::nbt::Value)>,
    pub phase: Option<Phase>,
    pub bootstrap: Option<Box<Bootstrap>>,
    pub teleport: Option<player::ResolvedTeleport>,
    pub player_motion: Option<world::java::player::Motion>,
    /// Native respawn retains rotation/velocity separately from the new world's unplaced body.
    respawn_rotation_velocity: Option<([f64; 3], f32, f32)>,
    /// Private correction anchor from a reused LocalPlayer; never published as a new-world pose.
    login_motion: Option<world::java::player::Motion>,
    pub player_dynamics: world::java::dynamics::PlayerDynamics,
    pub player_pose_revision: u64,
    pub deferred_entity_dynamics: u64,
    pending_correction: Option<u64>,
    pending_rotation: Option<(u64, u64, f32, f32)>,
    correction_sequence: u64,
    pub respawn: Option<Spawn>,
    pub keepalives: u64,
    pending_ping: Option<(u64, i32)>,
    ping_sequence: u64,
    pub deferred_packets: u64,
    pub deferred_types: std::collections::BTreeMap<String, u64>,
}
impl WorldState {
    pub fn install_block_registry(
        &mut self,
        registry: std::sync::Arc<assets::registry::java::BlockRegistry>,
    ) -> Result<(), &'static str> {
        assets::registry::java::prediction::builtin()
            .map_err(|_| "Invalid pinned Java block prediction facts")?;
        self.terrain.install_block_types(registry.block_types());
        piston::initialize(&mut self.terrain, &registry, None)?;
        self.piston_registry = Some(registry);
        Ok(())
    }
    /// Name from the same validated registry installed for this world, never a fallback.
    pub fn block_name(&self, state: u32) -> Option<&str> {
        self.piston_registry
            .as_ref()?
            .state(state)
            .map(|s| s.name.as_ref())
    }
    /// Admits each ACK restore and its native collision/snap before restoring the next cell.
    /// The application supplies its existing movement owner; no simulation enters world storage.
    pub fn admit_with_block_reconciliation(
        &mut self,
        envelope: Envelope,
        reconcile: impl FnMut(&mut Self, world::java::Reconciliation) -> Result<(), String>,
    ) -> bool {
        if let Event::BlockAck {
            dimension_epoch,
            sequence,
        } = envelope.event
        {
            if envelope.generation != self.generation
                || sequence < 0
                || !self.terrain.accepts(self.generation, dimension_epoch)
            {
                return false;
            }
            return self.reconcile_block_ack(dimension_epoch, sequence, reconcile);
        }
        self.admit(envelope)
    }
    /// Resumes an ACK paused on unavailable collision facts before later packets are admitted.
    pub fn resume_block_ack_reconciliation(
        &mut self,
        reconcile: impl FnMut(&mut Self, world::java::Reconciliation) -> Result<(), String>,
    ) -> bool {
        let Some((epoch, sequence)) = self.block_ack else {
            return true;
        };
        if !self.terrain.accepts(self.generation, epoch) {
            return true;
        }
        self.reconcile_block_ack(epoch, sequence, reconcile)
    }
    fn reconcile_block_ack(
        &mut self,
        epoch: u64,
        sequence: i32,
        mut reconcile: impl FnMut(&mut Self, world::java::Reconciliation) -> Result<(), String>,
    ) -> bool {
        self.block_ack = Some((epoch, sequence));
        for position in self.terrain.prediction_positions_up_to(sequence) {
            let Some(outcome) = self.terrain.reconcile_block(position, sequence) else {
                continue;
            };
            if outcome.changed {
                self.block_predictions_reverted += 1;
            } else {
                self.block_predictions_confirmed += 1;
            }
            if reconcile(self, outcome).is_err() {
                self.block_reconciliations.push_back(outcome);
                return false;
            }
        }
        true
    }
    pub fn tick_pistons(&mut self) -> Result<(), &'static str> {
        piston::tick(&mut self.terrain, self.piston_registry.as_deref())
    }
    pub fn begin(&mut self, generation: u64) {
        *self = Self {
            generation,
            players: world::java::roster::Roster::new(generation),
            ..Self::default()
        };
    }
    /// Domain-owned configured appearances for native resource preparation.
    pub fn pig_appearances(
        &self,
    ) -> Result<Vec<std::sync::Arc<world::java::entities::visual::PigAppearance>>, &'static str>
    {
        entities::appearances(self)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Matches AbstractClientPlayer's cached PlayerInfo, including removed tab entries.
    pub fn player_entity_mode(&self, id: i32) -> Option<world::java::hud::GameMode> {
        let entity = self.entities.get(id)?;
        let profile = entity.visual.player.as_ref()?;
        self.players
            .get(&entity.uuid)
            .filter(|entry| std::sync::Arc::ptr_eq(&entry.profile, profile))
            .map(|entry| entry.mode)
            .or_else(|| self.removed_player_modes.get(&entity.identity).copied())
    }
    /// Returns false for retired sessions before touching any admitted state.
    pub fn admit(&mut self, envelope: Envelope) -> bool {
        if envelope.generation != self.generation {
            return false;
        }
        match envelope.event {
            Event::CommandTree {
                nodes,
                message_arguments,
            } => self.chat.command_tree = Some((nodes, message_arguments)),
            Event::CommandPermission(level) => self.chat.command_permission = level,
            Event::CommandSuggestions(packet) => self.chat.suggestions = Some(packet.into()),
            Event::SystemChat(message) => self
                .chat
                .receive(message, self.inventory.cooldowns.client_tick()),
            Event::ChatCommandSent { sequence, written } => {
                self.chat.written = Some((sequence, written))
            }
            Event::SelectionSent {
                dimension_epoch,
                sequence,
                slot,
            } => {
                return self.inventory.selection_sent(
                    self.generation,
                    dimension_epoch,
                    sequence,
                    usize::from(slot),
                );
            }
            Event::InventorySent {
                dimension_epoch,
                incarnation,
                sequence,
                container,
                closed,
                written,
            } => {
                return self
                    .inventory
                    .record_receipt(::inventory::java::pickup::Receipt {
                        identity: ::inventory::java::pickup::MenuIdentity {
                            session: self.generation,
                            epoch: dimension_epoch,
                            container,
                            incarnation,
                        },
                        sequence,
                        closed,
                        written,
                    });
            }
            Event::DropWritten {
                dimension_epoch,
                sequence,
                wire_revision,
                slot,
                whole_stack,
                written,
            } => {
                return self.inventory.drop_written(
                    self.generation,
                    dimension_epoch,
                    ::inventory::java::DropReceipt {
                        sequence,
                        wire_revision,
                        slot: usize::from(slot),
                        whole_stack,
                        written,
                    },
                );
            }
            Event::Inventory {
                dimension_epoch,
                incarnation,
                wire_revision,
                packet,
            } => {
                if !self.inventory.accepts(self.generation, dimension_epoch) {
                    return false;
                }
                self.inventory.wire_revision = wire_revision;
                return inventory::admit(self, dimension_epoch, incarnation, packet);
            }
            Event::BlockActionWritten {
                dimension_epoch,
                sequence,
                written,
            } => {
                if sequence < 0 || !self.terrain.accepts(self.generation, dimension_epoch) {
                    return false;
                }
                self.inventory.block_use_written(
                    self.generation,
                    dimension_epoch,
                    sequence,
                    written,
                );
                if !written {
                    self.block_predictions_retired +=
                        self.terrain.discard_prediction(sequence) as u64;
                }
            }
            Event::BlockAck {
                dimension_epoch,
                sequence,
            } => {
                if sequence < 0 || !self.terrain.accepts(self.generation, dimension_epoch) {
                    return false;
                }
                return self.reconcile_block_ack(dimension_epoch, sequence, |state, outcome| {
                    state.block_reconciliations.push_back(outcome);
                    Ok(())
                });
            }
            Event::ServerTransfer { .. } => {}
            Event::Phase(phase) => {
                self.phase = Some(phase);
                if phase == Phase::Configuration {
                    self.chat.overlay = None;
                    self.scoreboard.clear();
                    self.inventory = Default::default();
                    self.block_ack = None;
                    self.block_reconciliations.clear();
                    self.status = Default::default();
                    self.bootstrap = None;
                    self.biomes.clear();
                    self.teleport = None;
                    self.player_motion = None;
                    self.login_motion = None;
                    self.player_dynamics = Default::default();
                    self.pending_correction = None;
                    self.pending_rotation = None;
                    self.pending_ping = None;
                    self.respawn_rotation_velocity = None;
                    self.death = None;
                    self.vitals = None;
                    self.terrain_error = None;
                    self.respawn = None;
                    self.terrain = world::java::Terrain::default();
                    self.entities = Default::default();
                    self.equipment.clear();
                    self.removed_player_modes.clear();
                    self.players = world::java::roster::Roster::new(self.generation);
                    self.player_visual = Default::default();
                }
            }
            Event::WorldLogin(world) => {
                self.chat.overlay = None;
                return world_login::admit(self, *world);
            }
            Event::Bootstrap(facts) => {
                if facts.generation != self.generation {
                    return false;
                }
                if let Some(entries) = facts
                    .configuration
                    .registries
                    .get("minecraft:worldgen/biome")
                {
                    match biomes::read(entries) {
                        Ok(biomes) => self.biomes = biomes,
                        Err(error) => {
                            self.terrain_error = Some(error);
                            return false;
                        }
                    }
                } else {
                    self.terrain_error = Some("Java biome registry missing");
                    return false;
                }
                let spawn = facts.world.spawn.clone();
                self.player_dynamics = dynamics::initial();
                self.bootstrap = Some(facts);
                if let Err(error) = terrain::begin(self, &spawn) {
                    self.terrain_error = Some(error);
                    return false;
                }
            }
            Event::Tags(tags) => {
                let Some(bootstrap) = self.bootstrap.as_mut() else {
                    return false;
                };
                if let Err(error) = bootstrap.configuration.update_tags(tags) {
                    self.terrain_error = Some(error);
                    return false;
                }
            }
            Event::Teleport {
                dimension_epoch,
                sequence,
                correction,
            } => match player::admit(self, dimension_epoch, sequence, correction) {
                Ok(accepted) => return accepted,
                Err(error) => {
                    self.terrain_error = Some(error);
                    return false;
                }
            },
            Event::Vitals(vitals) => self.vitals = Some(vitals),
            Event::Hud(packet) => hud::admit(self, packet),
            Event::Roster(packet) => {
                self.scoreboard.touch();
                return roster::admit(self, packet);
            }
            Event::Scoreboard(packet) => {
                if self.bootstrap.is_none() {
                    return false;
                }
                self.scoreboard.admit(packet);
            }
            Event::Entities {
                dimension_epoch,
                packet,
            } => match entities::admit(self, dimension_epoch, packet) {
                Ok(accepted) => return accepted,
                Err(error) => {
                    self.terrain_error = Some(error);
                    return false;
                }
            },
            Event::Dynamics {
                dimension_epoch,
                packet,
            } => match dynamics::admit(self, dimension_epoch, packet) {
                Ok(accepted) => return accepted,
                Err(error) => {
                    self.terrain_error = Some(error);
                    return false;
                }
            },
            Event::Death { player_id, message } => self.death = Some((player_id, message)),
            Event::Rotation {
                dimension_epoch,
                sequence,
                yaw,
                relative_yaw,
                pitch,
                relative_pitch,
            } => {
                if sequence <= self.correction_sequence {
                    return false;
                }
                match dynamics::admit(
                    self,
                    dimension_epoch,
                    java_protocol::session::dynamics::Packet::Rotation {
                        yaw,
                        relative_yaw,
                        pitch,
                        relative_pitch,
                    },
                ) {
                    Ok(false) => return false,
                    Err(error) => {
                        self.terrain_error = Some(error);
                        return false;
                    }
                    Ok(true) => {}
                }
                let motion = self.player_motion.expect("admitted rotation");
                self.correction_sequence = sequence;
                self.pending_rotation = Some((dimension_epoch, sequence, motion.yaw, motion.pitch));
            }
            Event::Respawn {
                spawn,
                keep_attribute_modifiers,
                keep_metadata,
            } => {
                self.chat.overlay = None;
                // ClientPacketListener.handleRespawn copies these only with KEEP_ENTITY_DATA.
                // Do not publish the old position as movement authority in the new dimension.
                let inherited = if keep_metadata {
                    self.player_motion
                        .map(|m| (m.velocity, m.yaw, m.pitch))
                        .or(self.respawn_rotation_velocity)
                        .unwrap_or(([0.; 3], -180., 0.))
                } else {
                    ([0.; 3], -180., 0.)
                };
                if !keep_attribute_modifiers {
                    for attribute in self.player_dynamics.attributes.values_mut() {
                        attribute.modifiers.clear();
                    }
                }
                if !keep_metadata {
                    self.player_dynamics.effects.clear();
                    self.player_visual = Default::default();
                }
                self.death = None;
                self.vitals = None;
                if let Err(error) = terrain::begin(self, &spawn) {
                    self.terrain_error = Some(error);
                    return false;
                }
                self.respawn_rotation_velocity = Some(inherited);
                self.respawn = Some(spawn);
            }
            Event::Border {
                dimension_epoch,
                packet,
            } => {
                if !self.terrain.accepts(self.generation, dimension_epoch) {
                    return false;
                }
                return self
                    .border
                    .apply((self.generation, dimension_epoch), packet);
            }
            Event::Terrain {
                dimension_epoch,
                packet,
            } => match terrain::admit(self, dimension_epoch, *packet) {
                Ok(accepted) => return accepted,
                Err(error) => {
                    self.terrain_error = Some(error);
                    return false;
                }
            },
            Event::Ping { sequence, id } => {
                if sequence <= self.ping_sequence {
                    return false;
                }
                self.ping_sequence = sequence;
                self.pending_ping = Some((sequence, id));
            }
            Event::KeepAlive(_) => {
                if self.phase == Some(Phase::Play) {
                    self.keepalives = self.keepalives.saturating_add(1);
                }
            }
            // These bytes were not decoded/admitted. Only a diagnostic count is retained.
            Event::DeferredPlay { name, .. } => {
                *self.deferred_types.entry(name).or_default() += 1;
                self.deferred_packets = self.deferred_packets.saturating_add(1)
            }
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_terrain_packet_cannot_cross_a_dimension_epoch() {
        let mut state = WorldState::default();
        state.begin(7);
        state.terrain.begin(world::java::Dimension {
            session: 7,
            epoch: 2,
            name: "minecraft:overworld".into(),
            min_y: -64,
            height: 384,
            has_skylight: true,
        });
        assert!(!state.admit(Envelope {
            generation: 7,
            event: Event::Terrain {
                dimension_epoch: 1,
                packet: Box::new(java_protocol::terrain::Packet::Center { x: 55, z: 33 })
            }
        }));
        assert_eq!(state.terrain.center, None);
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Terrain {
                dimension_epoch: 2,
                packet: Box::new(java_protocol::terrain::Packet::Center { x: -1, z: -2 })
            }
        }));
        assert_eq!(state.terrain.center, Some([-1, -2]));
        state.begin(8);
        assert!(state.terrain.is_empty());
        assert!(state.terrain.dimension().is_none());
    }
    #[test]
    fn old_generation_cannot_mutate_new_world() {
        let mut state = WorldState::default();
        state.begin(2);
        assert!(!state.admit(Envelope {
            generation: 1,
            event: Event::KeepAlive(123)
        }));
        assert_eq!(state.keepalives, 0);
        assert!(state.admit(Envelope {
            generation: 2,
            event: Event::KeepAlive(456)
        }));
        state.begin(3);
        assert_eq!(state.keepalives, 0);
        assert!(state.bootstrap.is_none());
    }
    #[test]
    fn configuration_keepalives_are_not_play_soak_evidence() {
        let mut state = WorldState::default();
        state.begin(1);
        for event in [Event::Phase(Phase::Configuration), Event::KeepAlive(1)] {
            state.admit(Envelope {
                generation: 1,
                event,
            });
        }
        assert_eq!(state.keepalives, 0);
        for event in [Event::Phase(Phase::Play), Event::KeepAlive(2)] {
            state.admit(Envelope {
                generation: 1,
                event,
            });
        }
        assert_eq!(state.keepalives, 1);
    }
}
