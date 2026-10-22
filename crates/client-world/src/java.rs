//! Java admission boundary. Initial authoritative facts stay distinct from Bedrock world state.
use java_protocol::session::{
    Envelope, Event, Phase,
    facts::{Bootstrap, Spawn, Teleport},
};
#[derive(Default, Debug)]
pub struct WorldState {
    generation: u64,
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
                    self.teleport = None;
                    self.respawn = None;
                }
            }
            Event::Bootstrap(facts) => {
                if facts.generation != self.generation {
                    return false;
                }
                self.bootstrap = Some(facts);
            }
            Event::Teleport(position) => self.teleport = Some(position),
            Event::Respawn(spawn) => self.respawn = Some(spawn),
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
