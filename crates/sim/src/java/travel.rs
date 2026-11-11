//! Native player travel for the pinned Java behavior references.
//!
//! Entity.getInputVector, LivingEntity.aiStep/travelInAir and Player edge protection supply
//! the observable rules. Context-dependent block callbacks remain explicit gaps.
mod bubbles;
mod climbing;
mod flight;
mod liquid;
mod slime;
mod sticky;
mod swimming;
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
    /// Native swimming flag, separate from the collision pose retained under low ceilings.
    pub swimming: bool,
    pub low_profile: bool,
    pub last_climbable_position: Option<[i32; 3]>,
    descending: bool,
    eyes_in_water: bool,
    pub fall_distance: f64,
    pub water_height: f64,
    pub lava_height: f64,
    jump_delay: u8,
    stuck_speed_multiplier: [f64; 3],
    flight_trigger: flight::Trigger,
}
impl Body {
    /// The last simulated shift-key state used by native entity collision contexts.
    pub fn descending(self) -> bool {
        self.descending
    }
    pub fn eyes_in_water(self) -> bool {
        self.eyes_in_water
    }
    pub fn eye_height(self, scale: f32) -> f64 {
        f64::from(
            if self.low_profile {
                0.4_f32
            } else if self.crouched {
                1.27_f32
            } else {
                1.62_f32
            } * scale,
        )
    }
    pub fn bounds(self, position: [f64; 3], scale: f32) -> Bounds {
        let mut shape = bounds(position, self.crouched, scale);
        if self.low_profile {
            shape.max[1] = position[1] + f64::from(0.6_f32 * scale);
        }
        shape
    }
}
#[derive(Clone, Copy)]
pub struct Parameters<'a> {
    pub speed: f32,
    pub sprint_speed: f32,
    pub friction_modifier: f32,
    pub air_drag_modifier: f32,
    pub gravity: f64,
    pub bounciness: f64,
    pub weaving: bool,
    pub jump_strength: f32,
    pub step_height: f32,
    pub scale: f32,
    pub sneaking_speed: f32,
    pub use_speed_multiplier: f32,
    pub movement_efficiency: f32,
    pub water_movement_efficiency: f32,
    pub fast_lava: bool,
    pub may_fly: bool,
    /// Some only while native abilities currently grant active flight.
    pub flying_speed: Option<f32>,
    /// Received speed also applies when a permitted native Jump edge starts flight.
    pub granted_flying_speed: f32,
    /// Native game-mode authority selects Entity.noPhysics independently of may_fly.
    pub spectator: bool,
    /// Current server block-tag members, never client fixture defaults.
    pub climbable: &'a [i32],
    pub suppresses_bounce: &'a [i32],
}
/// Ordered native ability changes around travel; the WorldState owner applies these facts.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightUpdates {
    pub input: Option<bool>,
    pub landing: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error("Java movement cannot yet apply {behavior} at {position:?}")]
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
    p: Parameters<'_>,
    terrain: &Terrain,
    physics: &Table,
    scope: (u64, u64),
) -> Result<FlightUpdates, Error> {
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
            p.use_speed_multiplier,
            p.movement_efficiency,
            p.water_movement_efficiency,
            p.granted_flying_speed,
        ]
        .into_iter()
        .all(f32::is_finite)
        || !p.gravity.is_finite()
        || !p.bounciness.is_finite()
        || p.flying_speed.is_some_and(|speed| !speed.is_finite())
        || p.scale <= 0.
        || p.step_height < 0.
    {
        return Err(Error::Invalid);
    }
    let mut next = *motion;
    let mut next_body = *body;
    next_body.descending = input.crouch;
    let mut flying = p.flying_speed.is_some();
    let flying_speed = p.flying_speed.unwrap_or(p.granted_flying_speed);
    let mut updates = FlightUpdates::default();
    if p.spectator {
        next_body.on_ground = false;
    }
    let cells = liquid::Cells { terrain, physics };
    // Entity.baseTick samples the previous pose and sprint state before LocalPlayer.aiStep.
    let previous_shape = body.bounds(next.position, p.scale);
    if !p.spectator {
        ensure_contacts(terrain, physics, previous_shape)?;
    }
    let contact = cells.contact(previous_shape)?;
    next_body.swimming = !flying
        && body.sprinting
        && contact.water > 0.
        && (body.swimming || (body.eyes_in_water && cells.water_at(next.position)?));
    next_body.eyes_in_water = cells.water_at_eye(next.position, body.eye_height(p.scale))?;
    let standing = bounds(next.position, false, p.scale);
    let blocked = if p.spectator {
        false
    } else {
        let nearby = collision::collect_player(
            terrain,
            physics,
            scope,
            standing,
            climbing::context(next.position, input.crouch),
        )?;
        nearby
            .boxes
            .iter()
            .any(|b| collision::intersects(standing, *b))
    };
    next_body.crouched = !flying
        && !next_body.swimming
        && (input.crouch || blocked)
        && swimming::fits(
            next.position,
            (false, true),
            p.scale,
            terrain,
            physics,
            scope,
            input.crouch,
        )?;
    next_body.sprinting = input.sprint
        && input.forward > 0.
        && (!next_body.crouched || body.eyes_in_water)
        && (!next_body.low_profile || contact.water > 0. || body.eyes_in_water)
        && (!body.horizontal_collision || next_body.swimming)
        && (flying || p.may_fly || contact.water <= 0. || body.eyes_in_water || next_body.swimming);
    let travel_pose = if flying || next_body.swimming || body.low_profile {
        *body
    } else {
        next_body
    };
    let shape = travel_pose.bounds(next.position, p.scale);
    next_body.water_height = contact.water;
    next_body.lava_height = contact.lava;
    if contact.water > 0. {
        next_body.fall_distance = 0.;
        if !flying {
            liquid::push(&mut next.velocity, contact.water_current, 0.014);
        }
    }
    if !flying && contact.lava > 0. {
        liquid::push(
            &mut next.velocity,
            contact.lava_current,
            if p.fast_lava {
                0.007
            } else {
                0.0023333333333333335
            },
        );
    }
    let support = if next_body.on_ground {
        collision::supporting_block(
            terrain,
            physics,
            scope,
            shape,
            next.position,
            climbing::context(next.position, input.crouch),
            None,
        )?
    } else {
        None
    };
    let feet = if p.spectator {
        physics.air().ok_or(Error::Invalid)?
    } else {
        surface_on(terrain, physics, next.position, -0.5, support)?
    };
    let inside = if p.spectator {
        feet
    } else {
        surface(terrain, physics, next.position, 0.)?
    };
    let block_friction = if next_body.on_ground {
        modified_friction(feet.friction, p.friction_modifier)
    } else {
        1.
    };
    let jump_power = p.jump_strength
        * if inside.jump_factor == 1. {
            feet.jump_factor
        } else {
            inside.jump_factor
        };
    updates.input = next_body.flight_trigger.tick(
        input.jump,
        p.may_fly,
        p.spectator,
        flying,
        next_body.swimming,
    );
    if let Some(enabled) = updates.input {
        flying = enabled;
        if enabled && next_body.on_ground && !p.spectator {
            jump_from_ground(&mut next, next_body.sprinting, jump_power);
        }
    }
    let in_water = !flying && contact.water > 0.;
    let in_lava = !flying && contact.lava > 0.;
    let wet = in_water || in_lava;
    next_body.jump_delay = next_body.jump_delay.saturating_sub(1);
    if flying {
        next_body.fall_distance = 0.;
        flight::lift(&mut next.velocity[1], input, flying_speed);
    }
    if next.velocity[0] * next.velocity[0] + next.velocity[2] * next.velocity[2] < 9.0e-6 {
        next.velocity[0] = 0.;
        next.velocity[2] = 0.;
    }
    if next.velocity[1].abs() < 0.003 {
        next.velocity[1] = 0.;
    }
    let threshold = if next_body.eye_height(p.scale) < 0.4 {
        0.
    } else {
        0.4
    };
    let fluid_jump = (contact.water > 0. && (!next_body.on_ground || contact.water > threshold))
        || (contact.lava > 0. && (!next_body.on_ground || contact.lava > threshold));
    if input.crouch && in_water {
        next.velocity[1] -= f64::from(0.04_f32);
    }
    if flying {
        next_body.jump_delay = 0;
    } else if input.jump && fluid_jump {
        next.velocity[1] += f64::from(0.04_f32);
    } else if input.jump && next_body.on_ground && next_body.jump_delay == 0 {
        jump_from_ground(&mut next, next_body.sprinting, jump_power);
        next_body.jump_delay = 10;
    } else if !input.jump {
        next_body.jump_delay = 0;
    }
    if next_body.swimming {
        swimming::steer(&mut next, input.jump, &cells)?;
    }
    let flight_y = next.velocity[1];
    let ground_speed = if next_body.sprinting {
        p.sprint_speed
    } else {
        p.speed
    };
    let mut water_drag = if next_body.sprinting {
        0.9_f32
    } else {
        0.8_f32
    };
    let speed = if wet {
        if contact.water > 0. {
            let efficiency =
                p.water_movement_efficiency * if next_body.on_ground { 1. } else { 0.5 };
            water_drag += (0.54600006_f32 - water_drag) * efficiency;
            0.02 + (ground_speed - 0.02) * efficiency
        } else {
            0.02
        }
    } else if next_body.on_ground {
        if f64::from(block_friction) > 0.6 {
            ground_speed * (0.21600002 / (block_friction * block_friction * block_friction))
        } else {
            ground_speed
        }
    } else if flying {
        flight::speed(flying_speed, next_body.sprinting)
    } else if next_body.sprinting {
        0.025999999
    } else {
        0.02
    };
    let axes = modified_input(
        [input.sideways, input.forward],
        p.use_speed_multiplier,
        if next_body.crouched || (next_body.low_profile && contact.water <= 0.) {
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
    if !flying
        && !wet
        && climbing::at(next.position, &mut next_body, p.climbable, terrain, physics)?
    {
        next_body.fall_distance = 0.;
        climbing::clamp(
            &mut next.velocity,
            input.crouch,
            climbing::is_scaffolding(next.position, terrain, physics)?,
        );
    }
    let was_falling = next.velocity[1] <= 0.;
    let movement = if p.spectator {
        next.velocity
    } else {
        sticky::consume(&mut next.velocity, &mut next_body.stuck_speed_multiplier)
    };
    let mut sweep = collision::expanded(shape, movement);
    sweep.max[1] += f64::from(p.step_height);
    sweep.min[1] -= f64::from(p.step_height) + 1.0e-7;
    let colliders = if p.spectator {
        collision::Query {
            boxes: vec![],
            cells: 0,
        }
    } else {
        collision::collect_player(
            terrain,
            physics,
            scope,
            sweep,
            climbing::context(next.position, input.crouch),
        )?
    };
    let old_y = next.position[1];
    let desired = if !flying && !p.spectator && input.crouch && movement[1] <= 0. {
        backed_off(shape, movement, next_body, p.step_height, &colliders.boxes)
    } else {
        movement
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
    next_body.fall_distance = if p.spectator {
        next_body.fall_distance
    } else if next_body.on_ground {
        0.
    } else {
        next_body.fall_distance + (-actual[1]).max(0.)
    };
    let support = if next_body.on_ground {
        collision::supporting_block(
            terrain,
            physics,
            scope,
            travel_pose.bounds(next.position, p.scale),
            next.position,
            climbing::context(next.position, input.crouch),
            Some(actual),
        )?
    } else {
        None
    };
    for i in [0, 2] {
        if (desired[i] - actual[i]).abs() >= f64::from(1.0e-5_f32) {
            next.velocity[i] *= if input.crouch { 0. } else { -p.bounciness };
        }
    }
    if desired[1] != actual[1] {
        let floor = surface_on(
            terrain,
            physics,
            next.position,
            -f64::from(0.2_f32),
            support,
        )?;
        let tagged = slime::suppressed(
            terrain,
            physics,
            next.position,
            support,
            p.suppresses_bounce,
        )?;
        next.velocity[1] = slime::restitute_y(
            next.velocity[1],
            actual[1],
            next_body.on_ground,
            input.crouch || (next_body.on_ground && tagged),
            if next_body.on_ground {
                p.bounciness.max(f64::from(floor.bounce_restitution))
            } else {
                p.bounciness
            },
            p.gravity,
            modified_friction(0.98, p.air_drag_modifier),
        );
    }
    if !flying
        && ((!wet && (next_body.horizontal_collision || input.jump))
            || (contact.water > 0. && next_body.horizontal_collision))
        && climbing::at(next.position, &mut next_body, p.climbable, terrain, physics)?
    {
        next.velocity[1] = 0.2;
    }
    let feet = if p.spectator {
        physics.air().ok_or(Error::Invalid)?
    } else {
        surface_on(terrain, physics, next.position, -0.5, support)?
    };
    let inside = if p.spectator {
        feet
    } else {
        surface(terrain, physics, next.position, 0.)?
    };
    if !p.spectator {
        ensure_contacts(terrain, physics, travel_pose.bounds(next.position, p.scale))?;
    }
    let cell = next.position.map(|n| n.floor() as i32);
    let water_block = physics
        .registry()
        .and_then(|r| terrain.block(cell).and_then(|id| r.state(id.0)))
        .is_some_and(|s| s.name.as_ref() == "minecraft:water");
    let water_block = water_block
        || terrain
            .block(cell)
            .is_some_and(|id| bubbles::is_column(physics, id.0));
    let surface_speed = if inside.speed_factor == 1. && !water_block {
        feet.speed_factor
    } else {
        inside.speed_factor
    };
    let speed_factor = if flying || p.spectator {
        1.
    } else {
        surface_speed + p.movement_efficiency * (1. - surface_speed)
    };
    let horizontal = block_friction * modified_friction(0.91, p.air_drag_modifier);
    for i in [0, 2] {
        next.velocity[i] *= f64::from(speed_factor);
        next.velocity[i] *= if in_water {
            f64::from(water_drag)
        } else if in_lava {
            0.5
        } else {
            f64::from(horizontal)
        };
    }
    if wet {
        if contact.water > 0. || contact.lava <= threshold {
            next.velocity[1] *= f64::from(0.8_f32);
            liquid::falling(
                &mut next.velocity,
                p.gravity,
                was_falling,
                next_body.sprinting,
            );
        } else {
            next.velocity[1] *= 0.5;
        }
        if contact.water <= 0. {
            next.velocity[1] -= p.gravity / 4.;
        }
        if next_body.horizontal_collision {
            let probe = collision::translated(
                travel_pose.bounds(next.position, p.scale),
                [
                    next.velocity[0],
                    next.velocity[1] + f64::from(0.6_f32) - next.position[1] + old_y,
                    next.velocity[2],
                ],
            );
            let query = collision::collect_player(
                terrain,
                physics,
                scope,
                probe,
                climbing::context(next.position, input.crouch),
            )?;
            if query.boxes.is_empty() && !cells.contains_liquid(probe)? {
                next.velocity[1] = f64::from(0.3_f32);
            }
        }
    } else {
        next.velocity[1] = (next.velocity[1] - p.gravity)
            * f64::from(modified_friction(0.98, p.air_drag_modifier));
    }
    if flying {
        next.velocity[1] = flight::damp(flight_y);
    }
    if !next.is_finite() {
        return Err(Error::Invalid);
    }
    if next_body.on_ground && !p.spectator {
        let cell = slime::contact_cell(next.position, support);
        if let Some(id) = climbing::cell(terrain, cell)?
            && slime::is_slime(physics, id)
        {
            slime::step_on(&mut next.velocity, input.crouch);
        }
    }
    if !p.spectator {
        bubbles::apply(
            motion.position,
            desired,
            &mut next,
            &mut next_body,
            travel_pose,
            p.scale,
            p.weaving,
            flying,
            terrain,
            physics,
        )?;
    }
    if flying && next_body.on_ground && !p.spectator {
        updates.landing = Some(false);
        flying = false;
    }
    if p.spectator {
        next_body.low_profile = false;
        next_body.crouched = false;
    } else {
        let pose_input = swimming::PoseInput {
            shift: input.crouch,
            crouch: input.crouch && !flying,
        };
        swimming::finish_pose(
            &mut next_body,
            next.position,
            pose_input,
            p.scale,
            terrain,
            physics,
            scope,
        )?;
    }
    *motion = next;
    *body = next_body;
    Ok(updates)
}

/// The same native jump applies to ordinary ground input and grounded flight takeoff.
fn jump_from_ground(motion: &mut Motion, sprinting: bool, power: f32) {
    if power > 1.0e-5 {
        motion.velocity[1] = motion.velocity[1].max(f64::from(power));
        if sprinting {
            let [sin, cos] = trig(motion.yaw * (std::f32::consts::PI / 180.));
            motion.velocity[0] -= sin * 0.2;
            motion.velocity[2] += cos * 0.2;
        }
    }
}

fn modified_friction(friction: f32, modifier: f32) -> f32 {
    (1. - (1. - friction) * modifier).clamp(0., 1.)
}

/// Keyboard input is normalized, then Java's square adjustment preserves diagonal reach.
fn modified_input(mut axes: [f32; 2], use_speed: f32, slow: f32) -> [f32; 2] {
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
        *v *= use_speed;
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
    surface_on(terrain, physics, position, y_offset, None)
}
fn surface_on<'a>(
    terrain: &Terrain,
    physics: &'a Table,
    position: [f64; 3],
    y_offset: f64,
    support: Option<[i32; 3]>,
) -> Result<&'a State, Error> {
    let cell = [
        support.map_or(position[0].floor() as i32, |s| s[0]),
        (position[1] + y_offset).floor() as i32,
        support.map_or(position[2].floor() as i32, |s| s[2]),
    ];
    let Some(id) = climbing::cell(terrain, cell)? else {
        return physics.air().ok_or(Error::Unsupported {
            behavior: "missing native air surface facts",
            position: cell,
        });
    };
    let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
    if state.special_callbacks
        && !bubbles::is_column(physics, id)
        && !bubbles::server_damage(physics, id)
        && !slime::is_slime(physics, id)
        && !sticky::supported(physics, id)
    {
        return Err(Error::Unsupported {
            behavior: "special block movement callbacks",
            position: cell,
        });
    }
    Ok(state)
}

fn ensure_contacts(terrain: &Terrain, physics: &Table, shape: Bounds) -> Result<(), Error> {
    for y in (shape.min[1] + 1.0e-7).floor() as i32..=(shape.max[1] - 1.0e-7).floor() as i32 {
        for z in (shape.min[2] + 1.0e-7).floor() as i32..=(shape.max[2] - 1.0e-7).floor() as i32 {
            for x in (shape.min[0] + 1.0e-7).floor() as i32..=(shape.max[0] - 1.0e-7).floor() as i32
            {
                let cell = [x, y, z];
                let Some(id) = climbing::cell(terrain, cell)? else {
                    continue;
                };
                let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
                if state.special_callbacks
                    && !bubbles::is_column(physics, id)
                    && !bubbles::server_damage(physics, id)
                    && !slime::is_slime(physics, id)
                    && !sticky::supported(physics, id)
                {
                    return Err(Error::Unsupported {
                        behavior: "special block contact behavior",
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
