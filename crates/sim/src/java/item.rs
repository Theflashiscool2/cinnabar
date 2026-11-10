//! Pinned ItemEntity dry motion. Unsupported block/fluid callbacks remain authoritative fallback.
use super::collision::{self, QueryError};
use assets::registry::java::physics::{Bounds, Collision, Table};
use world::java::{Terrain, entities::item_motion::View};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error("native item motion needs fluid, extended-shape or special block callbacks")]
    Unsupported,
}
impl Error {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unsupported => "native item fluid/shape/block callbacks not implemented",
            Self::Query(QueryError::Missing(_)) => "native item awaits neighbouring terrain",
            Self::Query(QueryError::Retired) => "retired native item world",
            Self::Query(QueryError::Dynamic { .. }) => {
                "native item dynamic collision not implemented"
            }
            Self::Query(QueryError::Unknown(_)) => "native item physics state missing",
            Self::Query(QueryError::Budget) => "native item collision budget exceeded",
            Self::Query(QueryError::Bounds) => "invalid native item collision bounds",
        }
    }
}
fn body(view: &View) -> Bounds {
    let radius = f64::from(view.dimensions[0] / 2.);
    Bounds {
        min: [
            view.position[0] - radius,
            view.position[1],
            view.position[2] - radius,
        ],
        max: [
            view.position[0] + radius,
            view.position[1] + f64::from(view.dimensions[1]),
            view.position[2] + radius,
        ],
    }
}
/// Transactional and bounded: an unavailable cell never commits speculative movement.
pub fn tick(
    terrain: &Terrain,
    physics: &Table,
    scope: (u64, u64),
    view: &mut View,
    network_id: i32,
    no_gravity: bool,
) -> Result<(), Error> {
    let mut next = view.clone();
    next.previous = next.position;
    next.ticks = next.ticks.wrapping_add(1);
    if !no_gravity {
        next.velocity[1] -= 0.04;
    }
    let shape = body(&next);
    let region = collision::expanded(shape, next.velocity);
    let query = collision::collect(terrain, physics, scope.0, scope.1, region)?;
    // ItemEntity.tick uses ordinary Entity.move clipping for dry static shapes, including slabs
    // and stairs. Extended shapes (fences/walls) also need getOnPos tag behavior; keep those,
    // fluids and block callbacks explicit until their native paths are implemented.
    let min = region.min.map(|p| p.floor() as i32 - 1);
    let max = region.max.map(|p| p.floor() as i32 + 1);
    let dimension = terrain.dimension().ok_or(QueryError::Retired)?;
    for y in min[1]..=max[1] {
        for z in min[2]..=max[2] {
            for x in min[0]..=max[0] {
                if y < dimension.min_y
                    || i64::from(y) >= i64::from(dimension.min_y) + i64::from(dimension.height)
                {
                    continue;
                }
                let pos = [x, y, z];
                let id = terrain.block(pos).ok_or(QueryError::Missing(pos))?.0;
                let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
                let ordinary = match &state.collision {
                    Collision::Static(boxes) => boxes.iter().all(|bounds| {
                        (0..3).all(|axis| bounds.min[axis] >= 0. && bounds.max[axis] <= 1.)
                    }),
                    Collision::Dynamic => false,
                };
                if !ordinary
                    || state.special_callbacks
                    || !state.fluid_empty
                    || state.speed_factor != 1.
                    || state.bounce_restitution != 0.
                {
                    return Err(Error::Unsupported);
                }
            }
        }
    }
    let moving = !next.on_ground
        || next.velocity[0].powi(2) + next.velocity[2].powi(2) > f64::from(1e-5_f32)
        || next.ticks.wrapping_add(network_id as u32).is_multiple_of(4);
    if moving {
        let requested = next.velocity;
        let actual = collision::clip(shape, requested, &query.boxes);
        next.position = std::array::from_fn(|i| next.position[i] + actual[i]);
        next.on_ground = requested[1] < 0. && actual[1] != requested[1];
        for i in 0..3 {
            if actual[i] != requested[i] {
                next.velocity[i] = 0.;
            }
        }
        let mut horizontal_drag = 0.98_f32;
        if next.on_ground {
            let support = collision::supporting_block(
                terrain,
                physics,
                scope,
                body(&next),
                next.position,
                collision::PlayerContext {
                    bottom: next.position[1],
                    descending: requested[1] < 0.,
                },
                Some(actual),
            )?;
            // Entity.getOnPos(.500001f) uses supporting X/Z but floors the adjusted Y for
            // ordinary blocks. Fence/wall/gate exceptions remain outside this dry subset.
            let pos = [
                support.map_or(next.position[0].floor() as i32, |position| position[0]),
                (next.position[1] - f64::from(0.500001_f32)).floor() as i32,
                support.map_or(next.position[2].floor() as i32, |position| position[2]),
            ];
            let id = terrain.block(pos).ok_or(QueryError::Missing(pos))?.0;
            horizontal_drag *= physics.state(id).ok_or(QueryError::Unknown(id))?.friction;
        }
        next.velocity[0] *= f64::from(horizontal_drag);
        next.velocity[1] *= f64::from(0.98_f32);
        next.velocity[2] *= f64::from(horizontal_drag);
        if next.on_ground && next.velocity[1] < 0. {
            next.velocity[1] *= -0.5;
        }
    }
    next.active = true;
    next.fallback = None;
    *view = next;
    Ok(())
}

#[cfg(test)]
mod tests;
