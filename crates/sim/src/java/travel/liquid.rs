//! Native fluid immersion and currents. References: pinned EntityFluidInteraction,
//! FlowingFluid.getFlow and LivingEntity.travelInWater/travelInLava (Java 26.3).
use super::*;
use assets::registry::java::fluids::{self, Kind};

#[derive(Clone, Copy, Default, Debug)]
pub struct Contact {
    pub water: f64,
    pub lava: f64,
    pub water_current: [f64; 3],
    pub lava_current: [f64; 3],
}
pub struct Cells<'a> {
    pub terrain: &'a Terrain,
    pub physics: &'a Table,
}
impl Cells<'_> {
    fn get(&self, p: [i32; 3]) -> Result<Option<&fluids::State>, Error> {
        let dimension = self.terrain.dimension().ok_or(QueryError::Retired)?;
        if p[1] < dimension.min_y
            || i64::from(p[1]) >= i64::from(dimension.min_y) + i64::from(dimension.height)
        {
            return Ok(None);
        }
        let id = self.terrain.block(p).ok_or(QueryError::Missing(p))?.0;
        let state = self.physics.state(id).ok_or(QueryError::Unknown(id))?;
        match self.physics.fluids() {
            Some(facts) => Ok(Some(facts.state(id).ok_or(QueryError::Unknown(id))?)),
            None if state.fluid_empty => Ok(None),
            None => Err(Error::Unsupported {
                behavior: "missing verified fluid travel facts",
                position: p,
            }),
        }
    }
    pub fn has_fluid_at(&self, p: [f64; 3]) -> Result<bool, Error> {
        Ok(self
            .get(p.map(|v| v.floor() as i32))?
            .is_some_and(|s| s.kind != Kind::Empty))
    }
    pub fn water_at(&self, p: [f64; 3]) -> Result<bool, Error> {
        Ok(self
            .get(p.map(|v| v.floor() as i32))?
            .is_some_and(|s| s.kind == Kind::Water))
    }
    /// Native FluidState.getHeightForCamera also fills a source under a sturdy ceiling.
    pub fn water_at_eye(&self, position: [f64; 3], eye_height: f64) -> Result<bool, Error> {
        let eye = position[1] + eye_height;
        let p = [
            position[0].floor() as i32,
            eye.floor() as i32,
            position[2].floor() as i32,
        ];
        let Some(state) = self.get(p)?.filter(|s| s.kind == Kind::Water) else {
            return Ok(false);
        };
        let above_p = offset(p, [0, 1, 0]);
        let above = self.get(above_p)?;
        let same = above.is_some_and(|s| s.kind == Kind::Water);
        let source = !state.falling && state.height == 8_f32 / 9.;
        let ceiling = if source && !same && above.is_some() {
            let id = self
                .terrain
                .block(above_p)
                .ok_or(QueryError::Missing(above_p))?
                .0;
            let mask = match self.physics.registry() {
                Some(r) => fluids::contacts::mask(r, id).ok_or(Error::Unsupported {
                    behavior: "missing fluid camera face facts",
                    position: above_p,
                })?,
                None if above.is_some_and(|s| !s.solid) => 0,
                None => {
                    return Err(Error::Unsupported {
                        behavior: "missing fluid camera face facts",
                        position: above_p,
                    });
                }
            };
            mask & (1 << 2) != 0
        } else {
            false
        };
        let height = if same || ceiling { 1. } else { state.height };
        Ok(eye <= f64::from(p[1]) + f64::from(height))
    }
    fn sturdy(&self, p: [i32; 3], kind: Kind, face: usize) -> Result<bool, Error> {
        if self.get(p)?.is_none_or(|s| s.kind == kind) {
            return Ok(false);
        }
        let id = self.terrain.block(p).ok_or(QueryError::Missing(p))?.0;
        let mask = self
            .physics
            .registry()
            .and_then(|r| fluids::contacts::mask(r, id))
            .ok_or(Error::Unsupported {
                behavior: "missing native current face facts",
                position: p,
            })?;
        Ok(mask & 128 == 0 && mask & (1 << face) != 0)
    }
    pub(super) fn flow(&self, state: &fluids::State, p: [i32; 3]) -> Result<[f64; 3], Error> {
        let directions = [
            ([0, 0, -1], 4),
            ([1, 0, 0], 1),
            ([0, 0, 1], 5),
            ([-1, 0, 0], 0),
        ];
        let mut vector = [0.; 3];
        for (d, _) in directions {
            let n = offset(p, d);
            let neighbor = self.get(n)?;
            if neighbor.is_some_and(|s| s.kind != Kind::Empty && s.kind != state.kind) {
                continue;
            }
            let mut height = neighbor.map_or(0., |s| s.height);
            let slope = if height > 0. {
                state.height - height
            } else if !neighbor.is_some_and(|s| s.flow_blocking) {
                let below = self.get(offset(n, [0, -1, 0]))?;
                height = below
                    .filter(|s| s.kind == state.kind)
                    .map_or(0., |s| s.height);
                if height > 0. {
                    state.height - (height - 0.8888889_f32)
                } else {
                    0.
                }
            } else {
                0.
            };
            vector[0] += f64::from(d[0] as f32 * slope);
            vector[2] += f64::from(d[2] as f32 * slope);
        }
        if state.falling {
            for (d, face) in directions {
                let n = offset(p, d);
                if self.sturdy(n, state.kind, face)?
                    || self.sturdy(offset(n, [0, 1, 0]), state.kind, face)?
                {
                    vector = normalize(vector);
                    vector[1] -= 6.;
                    break;
                }
            }
        }
        Ok(normalize(vector))
    }
    /// No retained per-position cache: every tick sees current authoritative palettes. Missing
    /// neighbors fail the whole prediction and automatically recover on arrival/replacement.
    pub fn contact(&self, bounds: Bounds) -> Result<Contact, Error> {
        let min = bounds.min.map(|n| (n + 0.001).floor() as i32);
        let max = bounds.max.map(|n| (n - 0.001).ceil() as i32 - 1);
        let cells = (0..3)
            .try_fold(1u64, |a, i| {
                a.checked_mul((i64::from(max[i]) - i64::from(min[i]) + 1).max(0) as u64)
            })
            .ok_or(QueryError::Budget)?;
        if cells > collision::MAX_QUERY_CELLS {
            return Err(QueryError::Budget.into());
        }
        let mut result = Contact::default();
        let mut counts = [0u32; 2];
        // Native scan order matters: shallow current scaling uses the maximum seen so far.
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    let p = [x, y, z];
                    let Some(state) = self.get(p)?.filter(|s| s.kind != Kind::Empty) else {
                        continue;
                    };
                    let height = if self
                        .get(offset(p, [0, 1, 0]))?
                        .is_some_and(|s| s.kind == state.kind)
                    {
                        1.
                    } else {
                        state.height
                    };
                    let surface = f64::from(y) + f64::from(height);
                    if surface < bounds.min[1] + 0.001 {
                        continue;
                    }
                    let (depth, current, count) = match state.kind {
                        Kind::Water => {
                            (&mut result.water, &mut result.water_current, &mut counts[0])
                        }
                        Kind::Lava => (&mut result.lava, &mut result.lava_current, &mut counts[1]),
                        Kind::Empty => unreachable!(),
                    };
                    *depth = depth.max(surface - bounds.min[1]);
                    let flow = self.flow(state, p)?;
                    let scale = if *depth < 0.4 { *depth } else { 1. };
                    for i in 0..3 {
                        current[i] += flow[i] * scale;
                    }
                    *count += 1;
                }
            }
        }
        for (current, count) in [
            (&mut result.water_current, counts[0]),
            (&mut result.lava_current, counts[1]),
        ] {
            if current.iter().map(|n| n * n).sum::<f64>() < f64::from(1.0e-5_f32) {
                *current = [0.; 3];
            } else if count > 0 {
                for n in current {
                    *n /= f64::from(count);
                }
            }
        }
        Ok(result)
    }
    pub fn contains_liquid(&self, shape: Bounds) -> Result<bool, Error> {
        // isFree uses containsAnyLiquid's cell occupancy, not the immersion surface test.
        let min = shape.min.map(|n| n.floor() as i32);
        let max = shape.max.map(|n| n.floor() as i32);
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    if self.get([x, y, z])?.is_some_and(|s| s.kind != Kind::Empty) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}
fn offset(p: [i32; 3], d: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| p[i] + d[i])
}
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let length = v.into_iter().map(|n| n * n).sum::<f64>().sqrt();
    if length < 1.0e-5 {
        [0.; 3]
    } else {
        v.map(|n| n / length)
    }
}
pub fn push(velocity: &mut [f64; 3], current: [f64; 3], speed: f64) {
    if current == [0.; 3] {
        return;
    }
    let mut force = current.map(|n| n * speed);
    if velocity[0].abs() < 0.003
        && velocity[2].abs() < 0.003
        && force.into_iter().map(|n| n * n).sum::<f64>().sqrt() < 0.0045000000000000005
    {
        force = normalize(force).map(|n| n * 0.0045000000000000005);
    }
    for i in 0..3 {
        velocity[i] += force[i];
    }
}
pub fn falling(velocity: &mut [f64; 3], gravity: f64, was_falling: bool, sprinting: bool) {
    if gravity != 0. && !sprinting {
        velocity[1] = if was_falling
            && (velocity[1] - 0.005).abs() >= 0.003
            && (velocity[1] - gravity / 16.).abs() < 0.003
        {
            -0.003
        } else {
            velocity[1] - gravity / 16.
        };
    }
}
