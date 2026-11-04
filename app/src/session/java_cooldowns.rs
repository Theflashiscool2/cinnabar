//! Advances server-admitted item cooldowns independently of input, rendering and open menus.
use super::SessionController;
use bevy::prelude::*;
use std::time::Duration;

#[derive(Resource, Default)]
pub(crate) struct Clock {
    session: Option<u64>,
    elapsed: Duration,
}
pub(crate) fn advance(
    time: Res<Time<Real>>,
    mut clock: ResMut<Clock>,
    mut controller: ResMut<SessionController>,
) {
    let session = controller
        .java
        .as_ref()
        .map(|_| controller.java_world.generation());
    if clock.session != session {
        *clock = Clock {
            session,
            ..Default::default()
        };
    }
    if session.is_none() {
        return;
    }
    clock.elapsed = clock.elapsed.saturating_add(time.delta());
    let ticks = (clock.elapsed.as_nanos() / simulation_clock::TICK_DURATION.as_nanos())
        .min(simulation_clock::MAX_TICKS_PER_FRAME as u128) as u32;
    clock.elapsed -= simulation_clock::TICK_DURATION * ticks;
    controller
        .java_world
        .inventory
        .cooldowns
        .advance(u64::from(ticks));
}
