//! Java admission boundary. Initial authoritative facts stay distinct from Bedrock world state.
mod biomes;
mod terrain;
use java_protocol::session::{
    Envelope, Event, Phase,
    facts::{Bootstrap, Spawn, Teleport},
};
#[derive(Default, Debug)]
pub struct WorldState {
    generation: u64,
    dimension_epoch: u64,
    pub terrain: world::java::Terrain,
    pub biomes: Vec<world::java::BiomeAppearance>,
    pub terrain_error: Option<&'static str>,
    pub decoded_chunks: u64,
    pub block_updates: u64,
    pub vitals: Option<java_protocol::session::facts::Vitals>,
    pub death: Option<(i32, java_protocol::nbt::Value)>,
    pub phase: Option<Phase>,
    pub bootstrap: Option<Box<Bootstrap>>,
    pub teleport: Option<Teleport>,
    pub respawn: Option<Spawn>,
    pub keepalives: u64,
    pub deferred_packets: u64,
    pub deferred_types: std::collections::BTreeMap<String, u64>,
}
impl WorldState {
    pub fn begin(&mut self, generation: u64) {
        *self = Self {
            generation,
            ..Self::default()
        };
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Returns false for retired sessions before touching any admitted state.
    pub fn admit(&mut self, envelope: Envelope) -> bool {
        if envelope.generation != self.generation {
            return false;
        }
        match envelope.event {
            Event::Phase(phase) => {
                self.phase = Some(phase);
                if phase == Phase::Configuration {
                    self.bootstrap = None;
                    self.biomes.clear();
                    self.teleport = None;
                    self.respawn = None;
                    self.terrain = world::java::Terrain::default();
                }
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
                self.bootstrap = Some(facts);
                if let Err(error) = terrain::begin(self, &spawn) {
                    self.terrain_error = Some(error);
                    return false;
                }
            }
            Event::Teleport(position) => self.teleport = Some(position),
            Event::Vitals(vitals) => self.vitals = Some(vitals),
            Event::Death { player_id, message } => self.death = Some((player_id, message)),
            Event::Respawn(spawn) => {
                self.death = None;
                self.vitals = None;
                if let Err(error) = terrain::begin(self, &spawn) {
                    self.terrain_error = Some(error);
                    return false;
                }
                self.respawn = Some(spawn);
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
