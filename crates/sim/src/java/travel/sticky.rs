//! HoneyBlock.entityInside, WebBlock.entityInside and Entity's deferred stuck movement.
use super::*;
use assets::registry::java::physics::callbacks::{self, Kind};

pub(super) fn kind(physics: &Table, id: u32) -> Option<Kind> {
    callbacks::kind(physics.registry()?, id)
}

pub(super) fn supported(physics: &Table, id: u32) -> bool {
    matches!(kind(physics, id), Some(Kind::Honey | Kind::Cobweb))
}

/// Consume the previous tick's contact multiplier before edge protection and collision.
pub(super) fn consume(velocity: &mut [f64; 3], multiplier: &mut [f64; 3]) -> [f64; 3] {
    let mut movement = *velocity;
    if multiplier.iter().map(|v| v * v).sum::<f64>() > 1.0e-7 {
        for i in 0..3 {
            movement[i] *= multiplier[i];
        }
        *multiplier = [0.; 3];
        *velocity = [0.; 3];
    }
    movement
}

pub(super) fn web(body: &mut Body, weaving: bool) {
    body.fall_distance = 0.;
    body.stuck_speed_multiplier = if weaving {
        [0.5, 0.25, 0.5]
    } else {
        [0.25, f64::from(0.05_f32), 0.25]
    };
}

/// Honey's callback operates after travel, undoing its fixed vanilla gravity and drag.
pub(super) fn honey(motion: &mut Motion, body: &mut Body, cell: [i32; 3], scale: f32) {
    let old_y = motion.velocity[1] / f64::from(0.98_f32) + 0.08;
    if body.on_ground || motion.position[1] > f64::from(cell[1]) + 0.9375 - 1.0e-7 || old_y >= -0.08
    {
        return;
    }
    let edge = 0.4375 + f64::from(WIDTH * scale / 2.);
    if (f64::from(cell[0]) + 0.5 - motion.position[0]).abs() + 1.0e-7 <= edge
        && (f64::from(cell[2]) + 0.5 - motion.position[2]).abs() + 1.0e-7 <= edge
    {
        return;
    }
    if old_y < -0.13 {
        let factor = -0.05 / old_y;
        motion.velocity[0] *= factor;
        motion.velocity[2] *= factor;
    }
    motion.velocity[1] = (-0.05 - 0.08) * f64::from(0.98_f32);
    body.fall_distance = 0.;
}
