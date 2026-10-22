//! Resolves protocol status into native domain facts at the admission boundary.
use super::WorldState;
use java_protocol::session::hud as wire;
use world::java::hud::{Experience, GameMode, Snapshot};

pub(super) fn mode(mode: wire::GameMode) -> GameMode {
    match mode {
        wire::GameMode::Survival => GameMode::Survival,
        wire::GameMode::Creative => GameMode::Creative,
        wire::GameMode::Adventure => GameMode::Adventure,
        wire::GameMode::Spectator => GameMode::Spectator,
    }
}
pub(super) fn admit(state: &mut WorldState, packet: wire::Packet) {
    match packet {
        wire::Packet::GameMode(value) => state.status.mode = Some(mode(value)),
        wire::Packet::Experience {
            progress,
            level,
            total,
        } => {
            if (0.0..=1.0).contains(&progress) && level >= 0 && total >= 0 {
                state.status.experience = Some(Experience {
                    progress,
                    level: level as u32,
                    total: total as u32,
                });
            } else {
                state.status.odd_values = state.status.odd_values.saturating_add(1);
            }
        }
        wire::Packet::Notification { .. } => {
            state.status.unimplemented_notifications =
                state.status.unimplemented_notifications.saturating_add(1)
        }
    }
}
impl WorldState {
    /// A read-only projection of current authority; callers cannot mutate the player through it.
    pub fn hud_snapshot(&self) -> Option<Snapshot> {
        let d = self.terrain.dimension()?;
        let attribute = |name| {
            self.player_dynamics
                .attributes
                .get(name)
                .and_then(|a| a.value().ok())
                .map(|v| v as f32)
        };
        Some(Snapshot {
            session: d.session,
            epoch: d.epoch,
            mode: self.status.mode,
            hardcore: self.bootstrap.as_ref()?.world.hardcore,
            health: self.vitals.as_ref().map(|v| v.health),
            maximum_health: attribute("minecraft:max_health"),
            food: self.vitals.as_ref().map(|v| v.food),
            saturation: self.vitals.as_ref().map(|v| v.saturation),
            armor: self
                .player_dynamics
                .attributes
                .get("minecraft:armor")
                .and_then(|a| a.value().ok()),
            experience: self.status.experience,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use java_protocol::session::{Envelope, Event};
    #[test]
    fn native_hud_status_is_authoritative_and_old_sessions_cannot_replace_it() {
        let mut state = super::super::tests_support::state();
        state.player_dynamics = super::super::dynamics::initial();
        let xp = || wire::Packet::Experience {
            progress: 0.5,
            level: 12,
            total: 300,
        };
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Hud(xp())
        }));
        assert_eq!(state.hud_snapshot().unwrap().experience.unwrap().level, 12);
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Vitals(java_protocol::session::facts::Vitals {
                health: 7.5,
                food: 6,
                saturation: 0.0
            })
        }));
        let snapshot = state.hud_snapshot().unwrap();
        assert_eq!(
            (snapshot.health, snapshot.food, snapshot.maximum_health),
            (Some(7.5), Some(6), Some(20.0))
        );
        assert!(!state.admit(Envelope {
            generation: 6,
            event: Event::Hud(wire::Packet::GameMode(wire::GameMode::Spectator))
        }));
        assert_eq!(state.hud_snapshot().unwrap(), snapshot);
        admit(
            &mut state,
            wire::Packet::Experience {
                progress: 2.0,
                level: -1,
                total: -1,
            },
        );
        assert_eq!(state.status.odd_values, 1);
        assert_eq!(state.hud_snapshot().unwrap(), snapshot);
        let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
        state.admit(Envelope {
            generation: 7,
            event: Event::Respawn {
                spawn,
                keep_attribute_modifiers: false,
                keep_metadata: false,
            },
        });
        let snapshot = state.hud_snapshot().unwrap();
        assert_eq!(snapshot.epoch, 2);
        assert!(snapshot.health.is_none() && snapshot.experience.is_none());
        assert_eq!(snapshot.mode, Some(GameMode::Survival));
        state.begin(8);
        assert!(state.hud_snapshot().is_none());
    }
}
