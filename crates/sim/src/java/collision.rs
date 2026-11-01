//! Protocol-pinned Java collision resolution in double precision.
//!
//! Axis ordering, contact tolerance and ascending step candidates follow the pinned vanilla
//! Entity.collide / Direction.axisStepOrder / VoxelShape.collide behavior references.
use assets::registry::java::physics::{Bounds, Collision, Table};
use world::java::Terrain;

const CONTACT: f64 = 1.0e-7;
/// Explicit foreground-work budget; exceeding it is an incomplete query, never empty terrain.
const MAX_QUERY_CELLS: u64 = 32768;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum QueryError {
    #[error("retired Java collision scope")]
    Retired,
    #[error("Java collision awaits terrain at {0:?}")]
    Missing([i32; 3]),
    #[error("Java collision state {id} at {position:?} requires dynamic shape handling")]
    Dynamic { id: u32, position: [i32; 3] },
    #[error("Java collision state {0} is missing from the pinned physics report")]
    Unknown(u32),
    #[error("Java collision query exceeds the foreground-work budget")]
    Budget,
    #[error("invalid Java collision bounds")]
    Bounds,
}

pub struct Query {
    pub boxes: Vec<Bounds>,
    pub cells: u64,
}

/// Reads authoritative native palettes only. Missing chunks and dynamic shapes are explicit.
pub fn collect(
    terrain: &Terrain,
    physics: &Table,
    session: u64,
    epoch: u64,
    region: Bounds,
) -> Result<Query, QueryError> {
    let dimension = terrain
        .dimension()
        .filter(|d| d.session == session && d.epoch == epoch)
        .ok_or(QueryError::Retired)?;
    if (0..3).any(|i| {
        !region.min[i].is_finite() || !region.max[i].is_finite() || region.min[i] >= region.max[i]
    }) {
        return Err(QueryError::Bounds);
    }
    let coordinate = |v: f64, offset: i32| {
        let v = v.floor();
        if v < f64::from(i32::MIN + 2) || v > f64::from(i32::MAX - 2) {
            return Err(QueryError::Bounds);
        }
        Ok(v as i32 + offset)
    };
    let min = [
        coordinate(region.min[0] - CONTACT, -1)?,
        coordinate(region.min[1] - CONTACT, -1)?,
        coordinate(region.min[2] - CONTACT, -1)?,
    ];
    let max = [
        coordinate(region.max[0] + CONTACT, 1)?,
        coordinate(region.max[1] + CONTACT, 1)?,
        coordinate(region.max[2] + CONTACT, 1)?,
    ];
    let cells = (0..3).try_fold(1u64, |volume, i| {
        volume
            .checked_mul((i64::from(max[i]) - i64::from(min[i]) + 1) as u64)
            .ok_or(QueryError::Budget)
    })?;
    if cells > MAX_QUERY_CELLS {
        return Err(QueryError::Budget);
    }
    let mut boxes = Vec::new();
    for y in min[1]..=max[1] {
        // Outside the registered vertical interval is void space, not a missing column.
        if i64::from(y) < i64::from(dimension.min_y)
            || i64::from(y) >= i64::from(dimension.min_y) + i64::from(dimension.height)
        {
            continue;
        }
        for z in min[2]..=max[2] {
            for x in min[0]..=max[0] {
                let position = [x, y, z];
                let id = terrain
                    .block(position)
                    .ok_or(QueryError::Missing(position))?
                    .0;
                let state = physics.state(id).ok_or(QueryError::Unknown(id))?;
                match &state.collision {
                    Collision::Dynamic => {
                        let shape = super::piston::collision::shape(terrain, physics, position)
                            .ok_or(QueryError::Dynamic { id, position })?;
                        for bounds in shape {
                            let bounds = translated(bounds, position.map(f64::from));
                            if intersects(bounds, region) {
                                boxes.push(bounds);
                            }
                        }
                    }
                    Collision::Static(shape) => {
                        for bounds in shape {
                            let bounds = translated(*bounds, position.map(f64::from));
                            if intersects(bounds, region) {
                                boxes.push(bounds);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(Query { boxes, cells })
}

pub fn translated(bounds: Bounds, delta: [f64; 3]) -> Bounds {
    Bounds {
        min: std::array::from_fn(|i| bounds.min[i] + delta[i]),
        max: std::array::from_fn(|i| bounds.max[i] + delta[i]),
    }
}
pub fn expanded(bounds: Bounds, delta: [f64; 3]) -> Bounds {
    Bounds {
        min: std::array::from_fn(|i| bounds.min[i] + delta[i].min(0.)),
        max: std::array::from_fn(|i| bounds.max[i] + delta[i].max(0.)),
    }
}
pub fn intersects(a: Bounds, b: Bounds) -> bool {
    (0..3).all(|i| a.min[i] < b.max[i] && a.max[i] > b.min[i])
}

/// Clips one move against already scoped colliders. Y resolves first, then the larger
/// horizontal displacement; exact axis ties resolve X before Z.
pub fn clip(body: Bounds, desired: [f64; 3], boxes: &[Bounds]) -> [f64; 3] {
    if boxes.is_empty() {
        return desired;
    }
    let axes = if desired[0].abs() < desired[2].abs() {
        [1, 2, 0]
    } else {
        [1, 0, 2]
    };
    let mut actual = [0.; 3];
    for axis in axes {
        let bounds = translated(body, actual);
        let mut amount = desired[axis];
        for obstacle in boxes {
            if amount.abs() < CONTACT {
                amount = 0.;
                break;
            }
            if (0..3).any(|i| {
                i != axis
                    && (bounds.max[i] <= obstacle.min[i] + CONTACT
                        || bounds.min[i] >= obstacle.max[i] - CONTACT)
            }) {
                continue;
            }
            if amount > 0. && bounds.max[axis] <= obstacle.min[axis] + CONTACT {
                amount = amount.min(obstacle.min[axis] - bounds.max[axis]);
            } else if amount < 0. && bounds.min[axis] >= obstacle.max[axis] - CONTACT {
                amount = amount.max(obstacle.max[axis] - bounds.min[axis]);
            }
        }
        actual[axis] = amount;
    }
    actual
}

/// Resolves stepping using actual surface heights, rather than always lifting a full step.
/// Callers collect colliders over the requested sweep plus the maximum step height.
pub fn stepped(
    body: Bounds,
    desired: [f64; 3],
    on_ground: bool,
    max_step: f32,
    boxes: &[Bounds],
) -> [f64; 3] {
    let clipped = clip(body, desired, boxes);
    let landed = desired[1] != clipped[1] && desired[1] < 0.;
    if max_step <= 0.
        || !(on_ground || landed)
        || (desired[0] == clipped[0] && desired[2] == clipped[2])
    {
        return clipped;
    }
    let grounded = if landed {
        translated(body, [0., clipped[1], 0.])
    } else {
        body
    };
    let skip = clipped[1] as f32;
    let mut heights = boxes
        .iter()
        .flat_map(|b| [b.min[1], b.max[1]])
        .map(|y| (y - grounded.min[1]) as f32)
        .filter(|y| *y >= 0. && *y <= max_step && *y != skip)
        .collect::<Vec<_>>();
    heights.sort_unstable_by(f32::total_cmp);
    heights.dedup();
    let horizontal = |v: [f64; 3]| v[0] * v[0] + v[2] * v[2];
    for height in heights {
        let mut candidate = clip(grounded, [desired[0], f64::from(height), desired[2]], boxes);
        if horizontal(candidate) > horizontal(clipped) {
            candidate[1] -= body.min[1] - grounded.min[1];
            return candidate;
        }
    }
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body() -> Bounds {
        Bounds {
            min: [-0.3, 0., -0.3],
            max: [0.3, 1.8, 0.3],
        }
    }
    #[test]
    fn contact_faces_and_large_coordinate_precision() {
        let floor = Bounds {
            min: [-2., -1., -2.],
            max: [2., 0., 2.],
        };
        let wall = Bounds {
            min: [1., 0., -2.],
            max: [2., 3., 2.],
        };
        assert_eq!(clip(body(), [2., -0.08, 0.], &[floor, wall]), [0.7, 0., 0.]);
        let shift = [16_777_216., 0., 0.];
        let shifted = translated(body(), shift);
        let clipped = clip(shifted, [0.125, -0.08, 0.], &[translated(floor, shift)]);
        assert_eq!(clipped, [0.125, 0., 0.]);
    }
    #[test]
    fn diagonal_axis_order_matches_pinned_vanilla_vectors() {
        // Evaluated by Entity.collideWithShapes from the pinned server archive.
        let floor = Bounds {
            min: [-2., -1., -2.],
            max: [3., 0., 3.],
        };
        let corner = Bounds {
            min: [0.5, 0., 0.5],
            max: [1.5, 3., 1.5],
        };
        for (requested, expected) in [
            ([1., -0.08, 2.], [1., 0., 2.]),
            ([1., -0.08, 1.], [1., 0., 0.2]),
            ([2., -0.08, 1.], [2., 0., 1.]),
        ] {
            assert_eq!(clip(body(), requested, &[floor, corner]), expected);
        }
    }

    #[test]
    fn stairs_use_surface_height_and_low_ceiling_prevents_step() {
        let floor = Bounds {
            min: [-2., -1., -2.],
            max: [3., 0., 2.],
        };
        let slab = Bounds {
            min: [0.5, 0., -1.],
            max: [1.5, 0.5, 1.],
        };
        assert_eq!(
            stepped(body(), [0.5, -0.08, 0.], true, 0.6, &[floor, slab]),
            [0.5, 0.5, 0.]
        );
        let ceiling = Bounds {
            min: [-1., 2., -1.],
            max: [2., 3., 1.],
        };
        assert_eq!(
            stepped(body(), [0.5, -0.08, 0.], true, 0.6, &[floor, slab, ceiling]),
            [0.2, 0., 0.]
        );
        assert_eq!(
            stepped(body(), [0.5, 0.1, 0.], false, 0.6, &[floor, slab]),
            [0.2, 0.1, 0.]
        );
    }
    #[test]
    fn empty_or_retired_terrain_is_never_a_collision_free_world() {
        use sha2::{Digest, Sha256};
        let mut bytes = b"CBJPHYS1\0\0\0\x01\0\0\0\0\0".to_vec();
        bytes.extend([
            0x3f, 0, 0, 0, 0x3f, 0x80, 0, 0, 0x3f, 0x80, 0, 0, 0, 0, 0, 0,
        ]);
        bytes.extend(b"\x01\x01\0\x0cfixture:none\0\0\0\0");
        let table = Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 1).unwrap();
        let mut terrain = Terrain::default();
        assert!(matches!(
            collect(&terrain, &table, 7, 1, body()),
            Err(QueryError::Retired)
        ));
        terrain.begin(world::java::Dimension {
            session: 7,
            epoch: 1,
            name: "fixture:world".into(),
            min_y: -64,
            height: 384,
            has_skylight: true,
        });
        assert!(matches!(
            collect(&terrain, &table, 7, 1, body()),
            Err(QueryError::Missing(_))
        ));
        assert!(matches!(
            collect(&terrain, &table, 7, 2, body()),
            Err(QueryError::Retired)
        ));
        assert!(
            collect(&terrain, &table, 7, 1, translated(body(), [0., -100., 0.]))
                .unwrap()
                .boxes
                .is_empty()
        );
    }
}
