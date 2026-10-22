//! Presentation-only native movement. Authoritative motion and delta anchors remain on Entity.
//! Behavior reference: the manifest-pinned runtime's SteppedInterpolationHandler.
use super::PositionStep;
use std::collections::VecDeque;

/// Provisional resident work budget, shared across admitted native interpolation tracks.
/// Exceeding it is a reported incompatibility, never a silently discarded movement packet.
pub const MAX_PENDING_STEPS: usize = 262_144;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}
impl Pose {
    pub fn finite(self) -> bool {
        self.position.into_iter().all(f64::is_finite)
            && self.yaw.is_finite()
            && self.pitch.is_finite()
    }
    pub fn lerp(self, to: Self, fraction: f32) -> Self {
        Self {
            position: std::array::from_fn(|i| {
                self.position[i] + (to.position[i] - self.position[i]) * f64::from(fraction)
            }),
            yaw: rotation(self.yaw, to.yaw, fraction),
            pitch: self.pitch + (to.pitch - self.pitch) * fraction,
        }
    }
}
impl From<super::Motion> for Pose {
    fn from(m: super::Motion) -> Self {
        Self {
            position: m.position,
            yaw: m.yaw,
            pitch: m.pitch,
        }
    }
}
pub(super) fn wrap(angle: f32) -> f32 {
    let value = angle % 360.;
    if value >= 180. {
        value - 360.
    } else if value < -180. {
        value + 360.
    } else {
        value
    }
}
pub(super) fn rotation(from: f32, to: f32, fraction: f32) -> f32 {
    from + wrap(to - from) * fraction
}
#[derive(Clone, Debug)]
struct Step {
    pose: Pose,
    ticks: i32,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub previous: Pose,
    pub current: Pose,
    target: Pose,
    anchor: Pose,
    interval: u32,
    steps: VecDeque<Step>,
    step_ticks: f32,
    remaining_ticks: f32,
    speed: f32,
}
impl Track {
    pub fn new(pose: Pose, interval: u32) -> Result<Self, &'static str> {
        if !pose.finite() || interval == 0 || interval > i32::MAX as u32 {
            return Err("invalid native interpolation initialization");
        }
        Ok(Self {
            previous: pose,
            current: pose,
            // The native handler's mutable target starts empty; a motion packet seeds it.
            target: Pose::default(),
            anchor: pose,
            interval,
            steps: VecDeque::new(),
            step_ticks: 0.,
            remaining_ticks: 0.,
            speed: 1.,
        })
    }
    pub fn pending_steps(&self) -> usize {
        self.steps.len()
    }
    /// Receives each ordered path, including multiple packets between rendered frames.
    /// The caller admits this into a replacement entity only after all validation succeeds.
    pub fn retarget(&mut self, target: Pose, path: &[PositionStep]) -> Result<(), &'static str> {
        if !target.finite()
            || path
                .iter()
                .any(|s| s.tick_offset <= 0 || !s.position.into_iter().all(f64::is_finite))
        {
            return Err("invalid native interpolation path");
        }
        let starting = if self.steps.is_empty() {
            self.current
        } else {
            self.target
        };
        if !self.target.lerp(target, 0.5).finite() || !starting.lerp(target, 0.5).finite() {
            return Err("native interpolation arithmetic overflow");
        }
        let mut previous = starting.position;
        for step in path {
            if (0..3).any(|i| !(step.position[i] - previous[i]).is_finite()) {
                return Err("native interpolation coordinate span overflow");
            }
            previous = step.position;
        }
        if !self.steps.is_empty() && target == self.target {
            return Ok(());
        }
        let linear = path.is_empty() || target.position == self.target.position;
        let count = if linear { 1 } else { path.len() };
        if self.steps.len().saturating_add(count) > MAX_PENDING_STEPS {
            return Err("native interpolation exceeds provisional resident work budget");
        }
        let total = if linear {
            self.interval as i32
        } else {
            path.iter()
                .try_fold(0i32, |sum, s| sum.checked_add(s.tick_offset))
                .ok_or("native interpolation tick span overflow")?
        };
        if !self.remaining_ticks.is_finite() || !(self.remaining_ticks + total as f32).is_finite() {
            return Err("native interpolation duration overflow");
        }
        if self.steps.is_empty() {
            self.anchor = self.current;
            self.step_ticks = 1.;
        }
        if linear {
            self.steps.push_back(Step {
                pose: target,
                ticks: total,
            });
        } else {
            let mut offset = 0;
            for step in path {
                offset += step.tick_offset;
                let fraction = offset as f32 / total as f32;
                let mut pose = self.target.lerp(target, fraction);
                pose.position = step.position;
                self.steps.push_back(Step {
                    pose,
                    ticks: step.tick_offset,
                });
            }
        }
        self.remaining_ticks += total as f32;
        self.target = target;
        Ok(())
    }
    /// Discontinuities retire every pending step before another target can be admitted.
    pub fn snap(&mut self, pose: Pose) {
        self.previous = pose;
        self.current = pose;
        self.target = pose;
        self.anchor = pose;
        self.steps.clear();
        self.step_ticks = 0.;
        self.remaining_ticks = 0.;
        self.speed = 1.;
    }
    pub fn tick(&mut self) {
        self.previous = self.current;
        if self.steps.is_empty() {
            self.remaining_ticks = 0.;
            self.speed = 1.;
            return;
        }
        self.current = loop {
            let Some(step) = self.steps.front() else {
                break self.target;
            };
            if self.step_ticks < step.ticks as f32 {
                break self
                    .anchor
                    .lerp(step.pose, self.step_ticks / step.ticks as f32);
            }
            self.step_ticks -= step.ticks as f32;
            self.anchor = self.steps.pop_front().expect("observed step").pose;
        };
        let target_speed = (self.remaining_ticks / self.interval as f32).max(1.);
        self.speed += (target_speed - self.speed) * (1. / self.interval as f32);
        let advance = if self.speed < self.remaining_ticks {
            self.speed
        } else {
            self.speed = 1.;
            self.remaining_ticks
        };
        self.step_ticks += advance;
        self.remaining_ticks -= advance;
    }
    pub fn sample(&self, fraction: f32) -> Pose {
        self.previous.lerp(self.current, fraction.clamp(0., 1.))
    }
}

#[cfg(test)]
mod tests;
