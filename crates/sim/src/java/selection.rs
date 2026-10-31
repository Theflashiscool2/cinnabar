//! Native outline picking. DDA and plane tie rules follow the pinned BlockGetter/VoxelShape.
use assets::registry::java::{
    interaction::{Shape, Table},
    physics::Bounds,
};
use world::java::Terrain;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}
impl Face {
    pub fn opposite(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::East => Self::West,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub block: [i32; 3],
    pub state: u32,
    pub position: [f64; 3],
    pub face: Face,
    pub inside: bool,
}
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
    #[error("Java selection belongs to a retired dimension")]
    Retired,
    #[error("invalid Java selection ray")]
    Ray,
    #[error("Java selection needs unloaded block {0:?}")]
    Missing([i32; 3]),
    #[error("unknown Java selection state {0}")]
    Unknown(u32),
    #[error("Java state {0} needs a contextual selection shape")]
    Contextual(u32),
}

/// Missing/contextual geometry blocks selection instead of permitting a hit through it.
pub fn pick(
    terrain: &Terrain,
    table: &Table,
    scope: (u64, u64),
    from: [f64; 3],
    to: [f64; 3],
) -> Result<Option<Hit>, Error> {
    pick_with_context(terrain, table, scope, from, to, |_, _| None)
}

/// Supplies a native single-box outline for supported dynamic hosts; unresolved hosts still fail.
pub fn pick_with_context(
    terrain: &Terrain,
    table: &Table,
    scope: (u64, u64),
    from: [f64; 3],
    to: [f64; 3],
    mut context: impl FnMut([i32; 3], u32) -> Option<Bounds>,
) -> Result<Option<Hit>, Error> {
    let d = terrain
        .dimension()
        .filter(|d| (d.session, d.epoch) == scope)
        .ok_or(Error::Retired)?;
    traverse(from, to, |block| {
        if i64::from(block[1]) < i64::from(d.min_y)
            || i64::from(block[1]) >= i64::from(d.min_y) + i64::from(d.height)
        {
            return Ok(None);
        }
        let id = terrain.block(block).ok_or(Error::Missing(block))?.0;
        let state = table.state(id).ok_or(Error::Unknown(id))?;
        let dynamic;
        let (outline, interaction): (&[Bounds], &[Bounds]) = match &state.shape {
            Shape::Static {
                outline,
                interaction,
            } => (outline, interaction),
            Shape::Contextual => {
                dynamic = context(block, id).ok_or(Error::Contextual(id))?;
                (std::slice::from_ref(&dynamic), &[])
            }
        };
        let Some(mut hit) = clip(outline, block, from, to) else {
            return Ok(None);
        };
        if let Some(other) = clip(interaction, block, from, to)
            && distance(from, other.position) < distance(from, hit.position)
        {
            hit.face = other.face;
        }
        hit.state = id;
        Ok(Some(hit))
    })
}
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

fn traverse<T>(
    from: [f64; 3],
    to: [f64; 3],
    mut visit: impl FnMut([i32; 3]) -> Result<Option<T>, Error>,
) -> Result<Option<T>, Error> {
    if from.into_iter().chain(to).any(|n| {
        !n.is_finite() || n.floor() <= f64::from(i32::MIN) || n.floor() >= f64::from(i32::MAX)
    }) {
        return Err(Error::Ray);
    }
    if from == to {
        return Ok(None);
    }
    let start = std::array::from_fn::<_, 3, _>(|i| from[i] + -1e-7 * (to[i] - from[i]));
    let end = std::array::from_fn::<_, 3, _>(|i| to[i] + -1e-7 * (from[i] - to[i]));
    let delta = std::array::from_fn::<_, 3, _>(|i| end[i] - start[i]);
    let mut block = start.map(|v| v.floor() as i32);
    if let Some(hit) = visit(block)? {
        return Ok(Some(hit));
    }
    let sign = delta.map(|v| {
        if v > 0. {
            1
        } else if v < 0. {
            -1
        } else {
            0
        }
    });
    let step = std::array::from_fn::<_, 3, _>(|i| {
        if sign[i] == 0 {
            f64::MAX
        } else {
            f64::from(sign[i]) / delta[i]
        }
    });
    let mut next = std::array::from_fn::<_, 3, _>(|i| {
        step[i]
            * if sign[i] > 0 {
                1. - (start[i] - start[i].floor())
            } else {
                start[i] - start[i].floor()
            }
    });
    while next.iter().any(|&n| n <= 1.) {
        let axis = if next[0] < next[1] {
            if next[0] < next[2] { 0 } else { 2 }
        } else if next[1] < next[2] {
            1
        } else {
            2
        };
        block[axis] = block[axis].checked_add(sign[axis]).ok_or(Error::Ray)?;
        next[axis] += step[axis];
        if let Some(hit) = visit(block)? {
            return Ok(Some(hit));
        }
    }
    Ok(None)
}

/// Clips the native ordered boxes, including the inside-origin convention and plane tie order.
pub fn clip(boxes: &[Bounds], block: [i32; 3], from: [f64; 3], to: [f64; 3]) -> Option<Hit> {
    let delta = std::array::from_fn::<_, 3, _>(|i| to[i] - from[i]);
    if delta.iter().map(|n| n * n).sum::<f64>() < 1e-7 {
        return None;
    }
    let test = std::array::from_fn::<_, 3, _>(|i| from[i] + delta[i] * 0.001);
    if boxes.iter().any(|b| {
        (0..3).all(|i| {
            test[i] - f64::from(block[i]) >= b.min[i] && test[i] - f64::from(block[i]) < b.max[i]
        })
    }) {
        // Native Direction scans Y, Z, X with strict comparisons after f32 narrowing.
        let delta = delta.map(|v| v as f32);
        let mut face = Face::North;
        let mut highest = f32::from_bits(1);
        for (value, direction) in [
            (-delta[1], Face::Down),
            (delta[1], Face::Up),
            (-delta[2], Face::North),
            (delta[2], Face::South),
            (-delta[0], Face::West),
            (delta[0], Face::East),
        ] {
            if value > highest {
                highest = value;
                face = direction;
            }
        }
        return Some(Hit {
            block,
            state: 0,
            position: test,
            face: face.opposite(),
            inside: true,
        });
    }
    let mut fraction = 1.;
    let mut face = None;
    for bounds in boxes {
        for axis in 0..3 {
            if delta[axis].abs() <= 1e-7 {
                continue;
            }
            let positive = delta[axis] > 0.;
            let plane = if positive {
                bounds.min[axis]
            } else {
                bounds.max[axis]
            } + f64::from(block[axis]);
            let t = (plane - from[axis]) / delta[axis];
            if t <= 0. || t >= fraction {
                continue;
            }
            if (0..3).filter(|&i| i != axis).all(|i| {
                let value = from[i] + t * delta[i] - f64::from(block[i]);
                bounds.min[i] - 1e-7 < value && value < bounds.max[i] + 1e-7
            }) {
                fraction = t;
                face = Some(match (axis, positive) {
                    (0, true) => Face::West,
                    (0, false) => Face::East,
                    (1, true) => Face::Down,
                    (1, false) => Face::Up,
                    (2, true) => Face::North,
                    _ => Face::South,
                });
            }
        }
    }
    Some(Hit {
        block,
        state: 0,
        position: std::array::from_fn(|i| from[i] + fraction * delta[i]),
        face: face?,
        inside: false,
    })
}
#[cfg(test)]
mod tests;
