//! Ordered BlockGetter movement visits, adapted from the MIT native Java reference.
//! Copyright/license: assets/licenses/Native-Java-Reference-MIT.txt.
use super::{Bounds, MAX_QUERY_CELLS, QueryError, translated};
use std::collections::HashSet;

pub(crate) fn axes(delta: [f64; 3]) -> [usize; 3] {
    if delta[0].abs() < delta[2].abs() {
        [1, 2, 0]
    } else {
        [1, 0, 2]
    }
}
struct Sweep {
    seen: HashSet<[i32; 3]>,
    hits: Vec<([i32; 3], i32)>,
    work: u64,
}
impl Sweep {
    fn spend(&mut self) -> Result<(), QueryError> {
        self.work += 1;
        if self.work > MAX_QUERY_CELLS {
            return Err(QueryError::Budget);
        }
        Ok(())
    }
    fn visit(
        &mut self,
        a: [i32; 3],
        b: [i32; 3],
        direction: Option<[f64; 3]>,
        step: i32,
    ) -> Result<(), QueryError> {
        let lo = std::array::from_fn::<_, 3, _>(|i| a[i].min(b[i]));
        let hi = std::array::from_fn::<_, 3, _>(|i| a[i].max(b[i]));
        let count = (0..3).try_fold(1u64, |n, i| {
            n.checked_mul((i64::from(hi[i]) - i64::from(lo[i]) + 1) as u64)
                .ok_or(QueryError::Budget)
        })?;
        if count > MAX_QUERY_CELLS {
            return Err(QueryError::Budget);
        }
        let order = direction
            .map(|d| {
                let [a, b, c] = axes(d);
                [c, b, a]
            })
            .unwrap_or([0, 1, 2]);
        let size = std::array::from_fn::<_, 3, _>(|i| i64::from(hi[i]) - i64::from(lo[i]) + 1);
        for n in 0..count {
            self.spend()?;
            let mut n = n as i64;
            let mut p = lo;
            for i in order {
                let offset = n % size[i];
                n /= size[i];
                p[i] = if direction.is_none_or(|d| d[i] >= 0.) {
                    lo[i] + offset as i32
                } else {
                    hi[i] - offset as i32
                };
            }
            if self.seen.insert(p) {
                self.hits.push((p, step));
            }
        }
        Ok(())
    }
}
/// Native visited-cell order for one movement segment. The box belongs to `to`.
/// Work overflow is explicit; callers commit no partial effects on a query failure.
pub(crate) fn visits(
    from: [f64; 3],
    to: [f64; 3],
    end: Bounds,
) -> Result<Vec<([i32; 3], i32)>, QueryError> {
    if from
        .into_iter()
        .chain(to)
        .chain(end.min)
        .chain(end.max)
        .any(|n| !n.is_finite() || n < f64::from(i32::MIN + 2) || n > f64::from(i32::MAX - 2))
    {
        return Err(QueryError::Bounds);
    }
    let delta = std::array::from_fn::<_, 3, _>(|i| to[i] - from[i]);
    let mut sweep = Sweep {
        seen: HashSet::new(),
        hits: Vec::new(),
        work: 0,
    };
    if delta.iter().map(|n| n * n).sum::<f64>() < f64::from(1e-5_f32 * 1e-5_f32) {
        sweep.visit(
            end.min.map(|n| n.floor() as i32),
            end.max.map(|n| n.floor() as i32),
            None,
            0,
        )?;
        return Ok(sweep.hits);
    }
    let start = translated(end, delta.map(|n| -n));
    sweep.visit(
        start.min.map(|n| n.floor() as i32),
        start.max.map(|n| n.floor() as i32),
        Some(delta),
        0,
    )?;
    let abs = delta.map(f64::abs);
    let [x, y, z] = delta.map(|n| if n >= 0. { 1. } else { -1. });
    let corner = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [-x, -z, y]
    } else if abs[1] <= abs[2] {
        [z, -y, -x]
    } else {
        [-y, x, -z]
    };
    let size = std::array::from_fn::<_, 3, _>(|i| end.max[i] - end.min[i]);
    let finish = std::array::from_fn::<_, 3, _>(|i| {
        (end.min[i] + end.max[i]) / 2. + size[i] * 0.5 * corner[i]
    });
    let origin = std::array::from_fn::<_, 3, _>(|i| finish[i] - delta[i]);
    let mut pos = origin.map(|n| n.floor() as i32);
    let sign = delta.map(|n| {
        if n == 0. {
            0
        } else if n > 0. {
            1
        } else {
            -1
        }
    });
    let increment = std::array::from_fn::<_, 3, _>(|i| {
        if sign[i] == 0 {
            f64::MAX
        } else {
            f64::from(sign[i]) / delta[i]
        }
    });
    let mut boundary = std::array::from_fn::<_, 3, _>(|i| {
        increment[i]
            * if sign[i] > 0 {
                1. - (origin[i] - origin[i].floor())
            } else {
                origin[i] - origin[i].floor()
            }
    });
    let mut step = 0;
    while boundary.iter().any(|v| *v <= 1.) {
        sweep.spend()?;
        let axis = if boundary[0] < boundary[1] {
            if boundary[0] < boundary[2] { 0 } else { 2 }
        } else if boundary[1] < boundary[2] {
            1
        } else {
            2
        };
        pos[axis] += sign[axis];
        boundary[axis] += increment[axis];
        let Some(point) = clip_cube(pos, origin, finish) else {
            continue;
        };
        step += 1;
        let opposite = std::array::from_fn(|i| {
            let point = point[i].clamp(
                f64::from(pos[i]) + f64::from(1e-5_f32),
                f64::from(pos[i]) + 1. - f64::from(1e-5_f32),
            );
            (point - size[i] * corner[i]).floor() as i32
        });
        sweep.visit(pos, opposite, Some(delta), step)?;
    }
    sweep.visit(
        end.min.map(|n| n.floor() as i32),
        end.max.map(|n| n.floor() as i32),
        Some(delta),
        step + 1,
    )?;
    Ok(sweep.hits)
}
fn clip_cube(p: [i32; 3], from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
    let d = std::array::from_fn::<_, 3, _>(|i| to[i] - from[i]);
    let mut best = 1.;
    let mut hit = false;
    for i in 0..3 {
        let plane = if d[i] > 1e-7 {
            f64::from(p[i])
        } else if d[i] < -1e-7 {
            f64::from(p[i]) + 1.
        } else {
            continue;
        };
        let t = (plane - from[i]) / d[i];
        if t > 0.
            && t < best
            && (0..3).filter(|j| *j != i).all(|j| {
                let v = from[j] + t * d[j];
                f64::from(p[j]) - 1e-7 < v && v < f64::from(p[j]) + 1. + 1e-7
            })
        {
            best = t;
            hit = true;
        }
    }
    hit.then(|| std::array::from_fn(|i| from[i] + d[i] * best))
}
