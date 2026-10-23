//! Read-only Java status projection into the existing UI. No synthetic Bedrock events.
use super::UiRuntime;
use ui::BoundedStat;
use world::java::hud::{DISPLAY_ARMOR_POINTS, FOOD_POINTS, Snapshot};

impl UiRuntime {
    /// Rejects retired player snapshots before changing any HUD or life-state value.
    pub fn publish_java_hud(&mut self, snapshot: Snapshot) -> bool {
        if snapshot.session != self.session_id
            || snapshot.health.is_some_and(|n| !n.is_finite())
            || snapshot.maximum_health.is_some_and(|n| !n.is_finite())
            || snapshot.armor.is_some_and(|n| !n.is_finite())
            || self
                .java_hud
                .is_some_and(|old| old.session == snapshot.session && old.epoch > snapshot.epoch)
        {
            return false;
        }
        if self.java_hud.is_some_and(|old| old.epoch != snapshot.epoch) {
            self.gameplay_hud.clear();
        }
        let health = snapshot.health.and_then(|current| {
            let maximum = snapshot.maximum_health?;
            // Native icons round points up; row capacity also covers health above a changed max.
            BoundedStat::new(
                points(current.max(0.0))?,
                points(maximum.max(current).max(1.0))?,
            )
        });
        if let Some(current) = snapshot.health {
            if current.is_finite() {
                self.publish_local_player_alive(current > 0.0);
            }
            // This shared flag means authoritative recovery, not permission to press Respawn.
            self.local_player_respawn_ready = false;
        } else {
            self.local_player_health_available = false;
            self.local_player_respawn_ready = false;
        }
        if let (Some(old), Some(new)) = (self.hud.health(), health)
            && new.current() < old.current()
        {
            self.note_player_hurt();
        }
        let food = snapshot
            .food
            .and_then(|n| u16::try_from(n).ok())
            .and_then(|n| BoundedStat::new(n, FOOD_POINTS));
        let armor = snapshot.armor.and_then(|n| {
            if !n.is_finite() || n < 0.0 {
                return None;
            }
            // The native armor accessor floors points; ten icons bound only their display.
            BoundedStat::new(
                (n.floor() as u16).min(DISPLAY_ARMOR_POINTS),
                DISPLAY_ARMOR_POINTS,
            )
        });
        self.hud.set_stats(health, food, armor, None);
        self.hud.set_absorption(None);
        match snapshot.experience {
            Some(xp) => self.hud.set_experience(xp.level, xp.progress),
            None => self.hud.clear_experience(),
        }
        self.gameplay_hud.set_hardcore(snapshot.hardcore);
        if let Some(saturation) = snapshot.saturation {
            self.gameplay_hud.set_saturation(saturation);
        }
        self.java_hud = Some(snapshot);
        true
    }
    /// Clears only a native projection; an ordinary Bedrock frame remains untouched.
    pub fn clear_java_hud(&mut self) {
        if self.java_hud.take().is_some() {
            self.hud.clear();
            self.gameplay_hud.clear();
            self.local_player_health_available = false;
            self.local_player_respawn_ready = false;
        }
    }
    pub fn java_hud(&self) -> Option<Snapshot> {
        self.java_hud
    }
    pub(super) fn hud_selected_slot(&self, player: &player_state::PlayerState) -> Option<u8> {
        self.java_hud
            .map_or_else(|| player.selected_hotbar_slot(), |hud| hud.selected_slot)
    }
    pub(super) fn shows_survival_stats(&self, player: &player_state::PlayerState) -> bool {
        self.java_hud.map_or_else(
            || player.facts.survival_stats_visible(),
            |hud| hud.mode.is_some_and(|mode| mode.survival_stats()),
        )
    }
    pub(super) fn mode_shows_hotbar(&self, player: &player_state::PlayerState) -> bool {
        self.java_hud.map_or_else(
            || {
                player
                    .facts
                    .player_game_mode()
                    .is_none_or(|mode| mode.shows_hotbar())
            },
            |hud| hud.mode.is_some_and(|mode| mode.shows_hotbar()),
        )
    }
}
fn points(value: f32) -> Option<u16> {
    (value.is_finite() && value >= 0.0 && value.ceil() <= f32::from(u16::MAX))
        .then_some(value.ceil() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::java::hud::{Experience, GameMode};
    fn snapshot() -> Snapshot {
        Snapshot {
            session: 7,
            epoch: 1,
            mode: Some(GameMode::Survival),
            hardcore: false,
            selected_slot: Some(0),
            health: Some(7.5),
            maximum_health: Some(20.0),
            food: Some(6),
            saturation: Some(0.0),
            armor: Some(6.8),
            experience: Some(Experience {
                level: 12,
                progress: 0.5,
                total: 300,
            }),
        }
    }
    #[test]
    fn native_hud_projection_preserves_points_modes_and_rejects_stale_lifetimes() {
        let mut ui = UiRuntime::new(7);
        let player = player_state::PlayerState::new(7);
        assert!(ui.publish_java_hud(snapshot()));
        assert_eq!(ui.hud.health(), BoundedStat::new(8, 20));
        assert_eq!(ui.hud.hunger(), BoundedStat::new(6, 20));
        assert_eq!(ui.hud.armor(), BoundedStat::new(6, 20));
        assert_eq!(ui.hud.experience().unwrap().level, 12);
        assert!(ui.shows_survival_stats(&player));
        let mut malformed = snapshot();
        malformed.health = Some(f32::NAN);
        assert!(!ui.publish_java_hud(malformed));
        assert_eq!(ui.hud.health(), BoundedStat::new(8, 20));
        let mut dead = snapshot();
        dead.health = Some(0.0);
        assert!(ui.publish_java_hud(dead));
        assert_eq!(ui.local_player_alive(), Some(false));
        assert!(
            !ui.local_player_respawn_ready(),
            "zero health must open death controls, not dismiss them"
        );
        let mut next = snapshot();
        next.mode = Some(GameMode::Spectator);
        next.epoch = 2;
        next.health = None;
        next.experience = None;
        assert!(ui.publish_java_hud(next));
        assert!(!ui.shows_survival_stats(&player) && !ui.mode_shows_hotbar(&player));
        assert!(ui.hud.health().is_none() && ui.hud.experience().is_none());
        assert!(!ui.publish_java_hud(snapshot()));
        assert_eq!(ui.java_hud(), Some(next));
        ui.begin_session(8);
        assert!(!ui.publish_java_hud(snapshot()));
        assert!(ui.java_hud().is_none() && ui.hud.health().is_none());
        ui.hud.set_health(BoundedStat::new(5, 20));
        ui.clear_java_hud();
        assert_eq!(
            ui.hud.health(),
            BoundedStat::new(5, 20),
            "native cleanup must not clear Bedrock state"
        );
    }
}
