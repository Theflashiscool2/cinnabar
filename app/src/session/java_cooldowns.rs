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
    pub(super) fn backlog_ticks(&self) -> u128 {
        self.elapsed.as_nanos() / simulation_clock::TICK_DURATION.as_nanos()
    }
    pub(super) fn fraction(&self) -> f32 {
        (self.elapsed.as_secs_f32() / simulation_clock::TICK_DURATION.as_secs_f32()).clamp(0., 1.)
    }
    pub(super) fn service(
        &mut self,
        world: &mut client_world::java::WorldState,
        now: Instant,
    ) -> u32 {
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
            world.entities.tick_presentation();
            world.tick_shulkers();
            world.player_dynamics.tick_effects();
            world.player_visual.swing.tick();
            world.player_visual.hurt_ticks = world.player_visual.hurt_ticks.saturating_sub(1);
        }
        ticks
    }
}
pub(crate) fn advance(
    mut clock: ResMut<Clock>,
    mut controller: ResMut<SessionController>,
    presentation: Option<Res<super::java_render::Presentation>>,
) {
    if controller.java.is_none() {
        *clock = Default::default();
        return;
    }
    let ticks = clock.service(&mut controller.java_world, Instant::now());
    let physics = presentation.as_ref().and_then(|p| p.service_physics());
    advance_items(&mut controller.java_world, physics.as_deref(), ticks);
}

/// Item visuals use the same elapsed ticks and world loan as network/player processing.
pub(super) fn advance_items(
    world: &mut client_world::java::WorldState,
    physics: Option<&::assets::registry::java::physics::Table>,
    ticks: u32,
) {
    if ticks == 0 {
        return;
    }
    let scope = world.terrain.dimension().map(|d| (d.session, d.epoch));
    for entity in world.entities.items_mut() {
        let view = entity
            .item_motion
            .as_mut()
            .expect("item presentation filter");
        let ready = world
            .equipment
            .ground(entity.identity)
            .and_then(|s| s.stack.as_ref())
            .is_some();
        let supported = physics
            .zip(scope)
            .filter(|_| ready && world.terrain_error.is_none());
        if let Some((physics, scope)) = supported {
            for _ in 0..ticks {
                if let Err(error) = sim::java::item::tick(
                    &world.terrain,
                    physics,
                    scope,
                    view,
                    entity.network_id,
                    entity.visual.no_gravity,
                ) {
                    view.suspend(entity.motion, entity.on_ground, error.reason());
                    break;
                }
            }
        } else {
            view.suspend(
                entity.motion,
                entity.on_ground,
                "awaiting native item data and physics",
            );
        }
    }
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
        world.player_visual.swing.start(
            false,
            world::java::entities::animation::swing::Kind::Whack,
            6,
            6,
        );
        let mut lid = world::java::shulker::Motion::default();
        lid.event(1);
        world.terrain.shulkers.insert(
            [0; 3],
            world::java::shulker::State {
                generation: 0,
                block_id: 0,
                states: std::sync::Arc::from([]),
                motion: lid,
            },
        );
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.service(&mut world, start);
        clock.service(&mut world, start + Duration::from_millis(125));
        assert_eq!(world.inventory.cooldowns.client_tick(), 2);
        assert_eq!(world.terrain.shulkers[&[0; 3]].motion.sample(1.), 0.2);
        assert!(
            (world.player_visual.swing.sample(1.).unwrap().progress - 1. / 6.).abs() < 0.000001
        );
        assert_eq!(world.player_dynamics.effects["minecraft:haste"].duration, 3);
        clock.service(&mut world, start + Duration::from_millis(125));
        assert_eq!(world.inventory.cooldowns.client_tick(), 2);
        assert_eq!(world.terrain.shulkers[&[0; 3]].motion.sample(1.), 0.2);
        clock.service(&mut world, start + Duration::from_millis(150));
        assert_eq!(world.inventory.cooldowns.client_tick(), 3);
        assert_eq!(world.terrain.shulkers[&[0; 3]].motion.sample(1.), 0.3);
        assert!(
            (world.player_visual.swing.sample(1.).unwrap().progress - 1. / 3.).abs() < 0.000001
        );
        world.begin(8);
        world.inventory.begin(8, 1);
        assert!(world.terrain.shulkers.is_empty());
        assert!(world.player_visual.swing.sample(1.).is_none());
        clock.service(&mut world, start + Duration::from_secs(5));
        assert_eq!(
            world.inventory.cooldowns.client_tick(),
            0,
            "old session backlog must not advance a new player's timers"
        );
    }
}

#[cfg(test)]
mod remote_tests {
    use super::*;
    #[test]
    fn remote_interpolation_and_hurt_expire_on_world_ticks_without_render_updates() {
        use world::java::{
            entities::{Spawn, visual::Visual},
            player::Motion,
        };
        let mut world = client_world::java::WorldState::default();
        world.begin(7);
        world.inventory.begin(7, 1);
        world.entities.begin(7, 1);
        world
            .entities
            .spawn(
                (7, 1),
                Spawn {
                    transport_lifetime: 1,
                    network_id: 42,
                    uuid: [1; 16],
                    kind: "minecraft:pig".into(),
                    motion: Motion::default(),
                    head_yaw: 0.,
                    object_data: 0,
                    visual: Visual {
                        hurt_ticks: 10,
                        ..Default::default()
                    },
                    interpolation_interval: Some(3),
                },
            )
            .unwrap();
        world
            .entities
            .update((7, 1), 42, |e| {
                e.motion.position = [3., 0., 0.];
                e.presentation
                    .as_mut()
                    .unwrap()
                    .retarget(e.motion.into(), &[])?;
                Ok(())
            })
            .unwrap();
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.service(&mut world, start);
        for tick in 1..=10 {
            clock.service(&mut world, start + Duration::from_millis(50 * tick));
            let e = world.entities.get(42).unwrap();
            let position = e.presentation.as_ref().unwrap().track.current.position[0];
            let expected = (tick as f64).min(3.);
            // Native lerp fractions are f32 even though world positions are f64.
            assert!((position - expected).abs() <= f64::from(f32::EPSILON) * 3.);

            assert_eq!(e.visual.hurt_ticks, 10 - tick as u8);
        }
        assert_eq!(world.inventory.cooldowns.client_tick(), 10);
        clock.service(&mut world, start + Duration::from_millis(500));
        assert_eq!(
            world.inventory.cooldowns.client_tick(),
            10,
            "no double tick on frame reclaim"
        );
    }
}
