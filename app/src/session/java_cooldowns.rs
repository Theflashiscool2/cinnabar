//! One native client-timer clock follows the world loan across frame boundaries.
//! Item cooldown/effect durations keep ticking when collision or rendering is waiting.
use super::SessionController;
use bevy::prelude::*;
use std::time::Duration;
use web_time::Instant;

#[derive(Resource, Default)]
pub(crate) struct Clock {
    session: Option<u64>,
    last: Option<Instant>,
    elapsed: Duration,
}
impl Clock {
    pub(super) fn service(&mut self, world: &mut client_world::java::WorldState, now: Instant) {
        let session = Some(world.generation());
        if self.session != session {
            *self = Self {
                session,
                ..Default::default()
            };
        }
        let elapsed = self
            .last
            .replace(now)
            .map_or(Duration::ZERO, |old| now.saturating_duration_since(old));
        self.elapsed = self.elapsed.saturating_add(elapsed);
        let ticks = (self.elapsed.as_nanos() / simulation_clock::TICK_DURATION.as_nanos())
            .min(simulation_clock::MAX_TICKS_PER_FRAME as u128) as u32;
        self.elapsed -= simulation_clock::TICK_DURATION * ticks;
        world.inventory.cooldowns.advance(u64::from(ticks));
        for _ in 0..ticks {
            world.player_dynamics.tick_effects();
        }
    }
}
pub(crate) fn advance(mut clock: ResMut<Clock>, mut controller: ResMut<SessionController>) {
    if controller.java.is_none() {
        *clock = Default::default();
        return;
    }
    clock.service(&mut controller.java_world, Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_and_service_consume_elapsed_once_and_do_not_require_movement_ticks() {
        let mut world = client_world::java::WorldState::default();
        world.begin(7);
        world.inventory.begin(7, 1);
        world
            .inventory
            .cooldowns
            .set("fixture:group".into(), 20)
            .unwrap();
        world.player_dynamics.effects.insert(
            "minecraft:haste".into(),
            world::java::dynamics::Effect {
                amplifier: 0,
                duration: 5,
                flags: 0,
            },
        );
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.service(&mut world, start);
        clock.service(&mut world, start + Duration::from_millis(125));
        assert_eq!(world.inventory.cooldowns.client_tick(), 2);
        assert_eq!(world.player_dynamics.effects["minecraft:haste"].duration, 3);
        clock.service(&mut world, start + Duration::from_millis(125));
        assert_eq!(world.inventory.cooldowns.client_tick(), 2);
        clock.service(&mut world, start + Duration::from_millis(150));
        assert_eq!(world.inventory.cooldowns.client_tick(), 3);
        world.begin(8);
        world.inventory.begin(8, 1);
        clock.service(&mut world, start + Duration::from_secs(5));
        assert_eq!(
            world.inventory.cooldowns.client_tick(),
            0,
            "old session backlog must not advance a new player's timers"
        );
    }
}
