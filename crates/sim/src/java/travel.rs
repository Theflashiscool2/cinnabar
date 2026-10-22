//! Native dry-land player travel for the pinned Java behavior references.
//!
//! Entity.getInputVector, LivingEntity.aiStep/travelInAir and Player edge protection supply
//! the observable rules. Fluids and context-dependent block callbacks remain explicit gaps.
use super::collision::{self, QueryError};
use assets::registry::java::physics::{Bounds, State, Table};
use world::java::{
    Terrain,
    player::{Motion, trig},
};

pub const TICKS_PER_SECOND: u32 = 20;
const WIDTH: f32 = 0.6;
const STANDING_HEIGHT: f32 = 1.8;
const CROUCH_HEIGHT: f32 = 1.5;

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Positive sideways is left; positive forward follows yaw zero toward positive Z.
    pub sideways: f32,
    pub forward: f32,
    pub jump: bool,
    pub crouch: bool,
    pub sprint: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub crouched: bool,
    pub sprinting: bool,
    pub fall_distance: f64,
    jump_delay: u8,
}
impl Body {
    pub fn eye_height(self, scale: f32) -> f64 {
        f64::from(if self.crouched { 1.27_f32 } else { 1.62_f32 } * scale)
    }
}
#[derive(Clone, Copy)]
pub struct Parameters {
    pub speed: f32,
    pub sprint_speed: f32,
    pub friction_modifier: f32,
    pub air_drag_modifier: f32,
    pub gravity: f64,
    pub jump_strength: f32,
    pub step_height: f32,
    pub scale: f32,
    pub sneaking_speed: f32,
    pub movement_efficiency: f32,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error("Java dry-land movement cannot yet apply {behavior} at {position:?}")]
    Unsupported {
        behavior: &'static str,
        position: [i32; 3],
    },
    #[error("invalid native Java player movement input")]
    Invalid,
}

pub fn bounds(position: [f64; 3], crouched: bool, scale: f32) -> Bounds {
    let half = f64::from(WIDTH * scale / 2.);
    let height = f64::from(
        if crouched {
            CROUCH_HEIGHT
        } else {
            STANDING_HEIGHT
        } * scale,
    );
    Bounds {
        min: [position[0] - half, position[1], position[2] - half],
        max: [position[0] + half, position[1] + height, position[2] + half],
    }
}

/// One simulation tick. Authority changes only after every terrain/behavior query succeeds.
pub fn step(
    motion: &mut Motion,
    body: &mut Body,
    input: Input,
    p: Parameters,
    terrain: &Terrain,
    physics: &Table,
    scope: (u64, u64),
) -> Result<(), Error> {
    if !motion.is_finite()
        || ![
            input.sideways,
            input.forward,
            p.speed,
            p.sprint_speed,
            p.friction_modifier,
            p.air_drag_modifier,
            p.jump_strength,
            p.step_height,
            p.scale,
            p.sneaking_speed,
            p.movement_efficiency,
        ]
        .into_iter()
        .all(f32::is_finite)
        || !p.gravity.is_finite()
        || p.scale <= 0.
        || p.step_height < 0.
    {
        return Err(Error::Invalid);
    }
    let mut next = *motion;
    let mut next_body = *body;
    let standing = bounds(next.position, false, p.scale);
    let nearby = collision::collect(terrain, physics, scope.0, scope.1, standing)?;
    next_body.crouched = input.crouch
        || nearby
            .boxes
            .iter()
            .any(|b| collision::intersects(standing, *b));
    next_body.sprinting = input.sprint && !next_body.crouched && input.forward > 0.;
    let shape = bounds(next.position, next_body.crouched, p.scale);
    ensure_dry(terrain, physics, shape)?;
    let feet = surface(terrain, physics, next.position, -0.5)?;
    let inside = surface(terrain, physics, next.position, 0.)?;
    let block_friction = if next_body.on_ground {
        modified_friction(feet.friction, p.friction_modifier)
    } else {
        1.
    };
    next_body.jump_delay = next_body.jump_delay.saturating_sub(1);
    if next.velocity[0] * next.velocity[0] + next.velocity[2] * next.velocity[2] < 9.0e-6 {
        next.velocity[0] = 0.;
        next.velocity[2] = 0.;
    }
    if next.velocity[1].abs() < 0.003 {
        next.velocity[1] = 0.;
    }
    if input.jump && next_body.on_ground && next_body.jump_delay == 0 {
        let factor = if inside.jump_factor == 1. {
            feet.jump_factor
        } else {
            inside.jump_factor
        };
        let jump = p.jump_strength * factor;
        if jump > 1.0e-5 {
            next.velocity[1] = next.velocity[1].max(f64::from(jump));
            if next_body.sprinting {
                let [sin, cos] = trig(next.yaw * (std::f32::consts::PI / 180.));
                next.velocity[0] -= sin * 0.2;
                next.velocity[2] += cos * 0.2;
            }
        }
        next_body.jump_delay = 10;
    } else if !input.jump {
        next_body.jump_delay = 0;
    }
    let ground_speed = if next_body.sprinting {
        p.sprint_speed
    } else {
        p.speed
    };
    let speed = if next_body.on_ground {
        if f64::from(block_friction) > 0.6 {
            ground_speed * (0.21600002 / (block_friction * block_friction * block_friction))
        } else {
            ground_speed
        }
    } else if next_body.sprinting {
        0.025999999
    } else {
        0.02
    };
    let axes = modified_input(
        [input.sideways, input.forward],
        if next_body.crouched {
            p.sneaking_speed
        } else {
            1.
        },
    );
    let acceleration = input_vector(
        [f64::from(axes[0]), 0., f64::from(axes[1])],
        speed,
        next.yaw,
    );
    for (v, a) in next.velocity.iter_mut().zip(acceleration) {
        *v += a;
    }
    let mut sweep = collision::expanded(shape, next.velocity);
    sweep.max[1] += f64::from(p.step_height);
    sweep.min[1] -= f64::from(p.step_height) + 1.0e-7;
    let colliders = collision::collect(terrain, physics, scope.0, scope.1, sweep)?;
    let desired = if input.crouch && next.velocity[1] <= 0. {
        backed_off(
            shape,
            next.velocity,
            next_body,
            p.step_height,
            &colliders.boxes,
        )
    } else {
        next.velocity
    };
    let actual = collision::stepped(
        shape,
        desired,
        next_body.on_ground,
        p.step_height,
        &colliders.boxes,
    );
    let squared = |v: [f64; 3]| v.into_iter().map(|n| n * n).sum::<f64>();
    if squared(actual) > 1.0e-7 || squared(desired) - squared(actual) < 1.0e-7 {
        for (v, a) in next.position.iter_mut().zip(actual) {
            *v += a;
        }
    }
    next_body.horizontal_collision = (desired[0] - actual[0]).abs() >= f64::from(1.0e-5_f32)
        || (desired[2] - actual[2]).abs() >= f64::from(1.0e-5_f32);
    next_body.on_ground = desired[1] != actual[1] && desired[1] < 0.;
    next_body.fall_distance = if next_body.on_ground {
        0.
    } else {
        next_body.fall_distance + (-actual[1]).max(0.)
    };
    for i in [0, 2] {
        if (desired[i] - actual[i]).abs() >= f64::from(1.0e-5_f32) {
            next.velocity[i] = 0.;
        }
    }
    if desired[1] != actual[1] {
        next.velocity[1] = 0.;
    }
    let feet = surface(terrain, physics, next.position, -0.5)?;
    let inside = surface(terrain, physics, next.position, 0.)?;
    ensure_dry(
        terrain,
        physics,
        bounds(next.position, next_body.crouched, p.scale),
    )?;
    let surface_speed = if inside.speed_factor == 1. {
        feet.speed_factor
    } else {
        inside.speed_factor
    };
    let speed_factor = surface_speed + p.movement_efficiency * (1. - surface_speed);
    let horizontal = block_friction * modified_friction(0.91, p.air_drag_modifier);
    for i in [0, 2] {
        next.velocity[i] *= f64::from(speed_factor);
        next.velocity[i] *= f64::from(horizontal);
    }
    next.velocity[1] =
        (next.velocity[1] - p.gravity) * f64::from(modified_friction(0.98, p.air_drag_modifier));
    if !next.is_finite() {
        return Err(Error::Invalid);
    }
    *motion = next;
    *body = next_body;
    Ok(())
}

fn modified_friction(friction: f32, modifier: f32) -> f32 {
    (1. - (1. - friction) * modifier).clamp(0., 1.)
}

/// Keyboard input is normalized, then Java's square adjustment preserves diagonal reach.
fn modified_input(mut axes: [f32; 2], slow: f32) -> [f32; 2] {
    let length = (axes[0] * axes[0] + axes[1] * axes[1]).sqrt();
    if length == 0. {
        return axes;
    }
    if length > 1. {
        for v in &mut axes {
            *v /= length;
        }
    }
    for v in &mut axes {
        *v *= 0.98;
        *v *= slow;
    }
    let length = (axes[0] * axes[0] + axes[1] * axes[1]).sqrt();
    if length <= 0. {
        return axes;
    }
    let direction = axes.map(|n| n * (1. / length));
    let [x, z] = direction.map(f32::abs);
    let tan = x.min(z) / x.max(z);
    let length = (length * (1. + tan * tan).sqrt()).min(1.);
    direction.map(|n| n * length)
}

fn input_vector(mut input: [f64; 3], speed: f32, yaw: f32) -> [f64; 3] {
    let length = input.into_iter().map(|n| n * n).sum::<f64>();
    if length < 1.0e-7 {
        return [0.; 3];
    }
    if length > 1. {
        for v in &mut input {
            *v /= length.sqrt();
        }
    }
    for v in &mut input {
        *v *= f64::from(speed);
    }
    let [sin, cos] = trig(yaw * (std::f32::consts::PI / 180.));
    [
        input[0] * cos - input[2] * sin,
        input[1],
        input[2] * cos + input[0] * sin,
    ]
}

fn surface<'a>(
    terrain: &Terrain,
    physics: &'a Table,
    position: [f64; 3],
    y_offset: f64,
) -> Result<&'a State, Error> {
    let cell = [
        position[0].floor() as i32,
        (position[1] + y_offset).floor() as i32,
        position[2].floor() as i32,
    ];
    let id = terrain.block(cell).ok_or(QueryError::Missing(cell))?.0;
    let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
    if state.special_callbacks || state.bounce_restitution != 0. {
        return Err(Error::Unsupported {
            behavior: "special block movement callbacks",
            position: cell,
        });
    }
    Ok(state)
}

fn ensure_dry(terrain: &Terrain, physics: &Table, shape: Bounds) -> Result<(), Error> {
    for y in (shape.min[1] + 1.0e-7).floor() as i32..=(shape.max[1] - 1.0e-7).floor() as i32 {
        for z in (shape.min[2] + 1.0e-7).floor() as i32..=(shape.max[2] - 1.0e-7).floor() as i32 {
            for x in (shape.min[0] + 1.0e-7).floor() as i32..=(shape.max[0] - 1.0e-7).floor() as i32
            {
                let cell = [x, y, z];
                let id = terrain.block(cell).ok_or(QueryError::Missing(cell))?.0;
                let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
                if !state.fluid_empty || state.special_callbacks {
                    return Err(Error::Unsupported {
                        behavior: "fluid or block contact behavior",
                        position: cell,
                    });
                }
            }
        }
    }
    Ok(())
}

fn backed_off(
    shape: Bounds,
    mut delta: [f64; 3],
    body: Body,
    step: f32,
    boxes: &[Bounds],
) -> [f64; 3] {
    let can_fall = |x: f64, z: f64, height: f64| {
        let probe = Bounds {
            min: [
                shape.min[0] + 1.0e-7 + x,
                shape.min[1] - height - 1.0e-7,
                shape.min[2] + 1.0e-7 + z,
            ],
            max: [
                shape.max[0] - 1.0e-7 + x,
                shape.min[1],
                shape.max[2] - 1.0e-7 + z,
            ],
        };
        !boxes.iter().any(|b| collision::intersects(probe, *b))
    };
    let step = f64::from(step);
    if !(body.on_ground
        || body.fall_distance < step && !can_fall(0., 0., step - body.fall_distance))
    {
        return delta;
    }
    let reduce = |v: f64| {
        if v.abs() <= 0.05 {
            0.
        } else {
            v - v.signum() * 0.05
        }
    };
    while delta[0] != 0. && can_fall(delta[0], 0., step) {
        delta[0] = reduce(delta[0]);
    }
    while delta[2] != 0. && can_fall(0., delta[2], step) {
        delta[2] = reduce(delta[2]);
    }
    while delta[0] != 0. && delta[2] != 0. && can_fall(delta[0], delta[2], step) {
        delta[0] = reduce(delta[0]);
        delta[2] = reduce(delta[2]);
    }
    delta
}

#[cfg(test)]
mod tests;
