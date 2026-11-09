//! Lends the sole Java authority and simulation to a worker while the frame is rendering.
//! Like the world-stream service, it returns ownership before First; extraction reads only
//! already-published presentation. No second inventory, world or player prediction is created.
use super::{SessionController, java_movement::Movement, java_render::Presentation};
use bevy::prelude::*;
use client_runtime::world_service::{WorldServiceLaunch, WorldServiceReclaim};
use std::sync::{Arc, Mutex};
mod worker;
use std::time::Duration;
use web_time::Instant;
use worker::Worker;

struct Authority {
    java: client_session::java::Session,
    world: client_world::java::WorldState,
    movement: Movement,
    clock: super::java_cooldowns::Clock,
    interaction: super::java_interaction::Interaction,
    shapes: Option<Arc<assets::registry::java::interaction::Table>>,
    physics: Option<Arc<assets::registry::java::physics::Table>>,
    scope: Option<(u64, u64)>,
}
impl worker::Work for Authority {
    fn service(&mut self) {
        self.slice(Instant::now());
    }
}
impl Authority {
    fn slice(&mut self, now: Instant) {
        for _ in 0..64 {
            let Some(event) = super::java::poll_event(&mut self.java, &mut self.world) else {
                break;
            };
            if !self.world.admit(event) {
                self.java.diagnostic_notice("world_event_not_admitted");
            }
        }
        let visual_ticks = self.clock.service(&mut self.world, now);
        let elapsed = self.movement.elapsed(&self.world, now);
        let current = self.world.terrain.dimension().map(|d| (d.session, d.epoch));
        // A replacement world requires its own real PlayerLoaded/publication admission.
        // Corrections in the admitted world replace kinematics and fence queued old input.
        let physics = self.physics.as_deref().filter(|_| {
            self.scope == current && self.world.terrain_error.is_none() && !self.java.is_finished()
        });
        super::java_cooldowns::advance_items(&mut self.world, physics, visual_ticks);
        let corrected = self.movement.0.begin_frame(
            &self.world,
            if physics.is_some() {
                elapsed
            } else {
                Duration::ZERO
            },
        );
        self.movement.1.corrected |= corrected;
        if corrected && let Some(motion) = self.world.player_motion {
            self.movement.1.controls.yaw = motion.yaw;
            self.movement.1.controls.pitch = motion.pitch;
        }
        let release_use =
            !self.interaction.1.use_button_down() || self.world.inventory.opened.is_some();
        if !self
            .interaction
            .prepare_movement(&self.java, &mut self.world, release_use)
        {
            return;
        }
        self.movement.0.use_effects = self.interaction.0.use_effects();
        if let Some(physics) = physics {
            let before = self.movement.0.ticks;
            super::java_movement::advance(&self.java, &mut self.world, physics, &mut self.movement);
            self.movement.1.ticks += self.movement.0.ticks.saturating_sub(before);
            self.interaction.service(
                &self.java,
                &mut self.world,
                &self.movement,
                self.shapes.as_deref(),
                physics,
            );
        }
    }
}

#[derive(Resource, Default)]
struct Slot(Mutex<Option<Worker<Authority>>>);

pub(crate) fn configure(app: &mut App) {
    app.init_resource::<Slot>()
        .add_systems(WorldServiceLaunch, launch)
        .add_systems(WorldServiceReclaim, reclaim);
}
fn launch(
    slot: Res<Slot>,
    controller: Option<ResMut<SessionController>>,
    movement: Option<ResMut<Movement>>,
    clock: Option<ResMut<super::java_cooldowns::Clock>>,
    interaction: Option<ResMut<super::java_interaction::Interaction>>,
    presentation: Option<Res<Presentation>>,
) {
    let (
        Some(mut controller),
        Some(mut movement),
        Some(mut clock),
        Some(mut interaction),
        Some(presentation),
    ) = (controller, movement, clock, interaction, presentation)
    else {
        return;
    };
    if controller.java.is_none() {
        return;
    }
    let mut worker = slot.0.lock().unwrap();
    if worker.is_none() {
        match Worker::spawn() {
            Ok(created) => *worker = Some(created),
            Err(error) => {
                bevy::log::error!(%error, "Java game service could not start");
                return;
            }
        }
    }
    let physics = presentation.service_physics();
    let scope = physics.as_ref().and_then(|_| {
        controller
            .java_world
            .terrain
            .dimension()
            .map(|d| (d.session, d.epoch))
    });
    worker.as_mut().unwrap().launch(Authority {
        java: controller.java.take().unwrap(),
        world: std::mem::take(&mut controller.java_world),
        movement: std::mem::take(&mut *movement),
        clock: std::mem::take(&mut *clock),
        interaction: std::mem::take(&mut *interaction),
        shapes: presentation.service_interaction(),
        physics,
        scope,
    });
}
fn reclaim(
    slot: Res<Slot>,
    controller: Option<ResMut<SessionController>>,
    movement: Option<ResMut<Movement>>,
    clock: Option<ResMut<super::java_cooldowns::Clock>>,
    interaction: Option<ResMut<super::java_interaction::Interaction>>,
) {
    let Some(authority) = slot.0.lock().unwrap().as_mut().and_then(Worker::take) else {
        return;
    };
    let (Some(mut controller), Some(mut movement), Some(mut clock), Some(mut interaction)) =
        (controller, movement, clock, interaction)
    else {
        return;
    };
    assert!(
        controller.java.is_none(),
        "Java session replaced while lent"
    );
    controller.java = Some(authority.java);
    controller.java_world = authority.world;
    *movement = authority.movement;
    *clock = authority.clock;
    *interaction = authority.interaction;
}
