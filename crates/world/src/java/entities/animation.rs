//! Native pig and unarmed player presentation with separate body controllers.
//! These sampled transforms never replace the entity's authoritative movement or collision.
mod player;

use super::{
    PositionStep,
    interpolation::{Pose, Track, rotation, wrap},
};

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub pose: Pose,
    pub body_yaw: f32,
    pub head_yaw: f32,
    pub walk_position: f32,
    pub walk_speed: f32,
    pub age_in_ticks: f32,
}
impl Sample {
    /// Quadruped model rotations, in radians: head, right/left hind, right/left front.
    pub fn rotations(self) -> [[f32; 3]; 5] {
        let phase = self.walk_position * 0.6662;
        let amplitude = |angle| super::super::player::trig(angle)[1] as f32 * 1.4 * self.walk_speed;
        let a = amplitude(phase);
        let b = amplitude(phase + std::f32::consts::PI);
        [
            [
                self.pose.pitch.to_radians(),
                wrap(self.head_yaw - self.body_yaw).to_radians(),
                0.,
            ],
            [a, 0., 0.],
            [b, 0., 0.],
            [b, 0., 0.],
            [a, 0., 0.],
        ]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Pig,
    Player,
}
#[derive(Clone, Debug)]
pub struct Presentation {
    kind: BodyKind,
    ticks: u64,
    pub track: Track,
    head: f32,
    old_head: f32,
    head_target: f32,
    head_steps: u8,
    body: f32,
    old_body: f32,
    stable_head: f32,
    stable_ticks: u32,
    walk: Walk,
}
#[derive(Clone, Debug, Default)]
struct Walk {
    old_speed: f32,
    speed: f32,
    position: f32,
    scale: f32,
}
impl Walk {
    fn update(&mut self, distance: f32, baby: bool) {
        self.old_speed = self.speed;
        self.speed += ((distance * 4.).min(1.) - self.speed) * 0.4;
        self.position += self.speed;
        self.scale = if baby { 3. } else { 1. };
    }
}
impl Presentation {
    pub fn new(pose: Pose, head: f32, interval: u32, kind: BodyKind) -> Result<Self, &'static str> {
        if !head.is_finite() {
            return Err("nonfinite native head angle");
        }
        Ok(Self {
            kind,
            ticks: 0,
            track: Track::new(pose, interval)?,
            head,
            old_head: head,
            head_target: head,
            head_steps: 0,
            body: head,
            old_body: head,
            stable_head: 0.,
            stable_ticks: 0,
            walk: Walk::default(),
        })
    }
    pub fn retarget(&mut self, pose: Pose, path: &[PositionStep]) -> Result<(), &'static str> {
        self.track.retarget(pose, path)
    }
    pub fn head_to(&mut self, head: f32) {
        self.head_target = head;
        self.head_steps = 3;
    }
    pub fn snap(&mut self, pose: Pose) {
        self.track.snap(pose);
        self.walk = Walk::default();
        self.old_head = self.head;
        self.old_body = self.body;
    }
    /// One standard world tick. Mounted/controlled entity simulation is intentionally separate.
    pub fn tick(&mut self, baby: bool, alive: bool, passenger: bool, carrying: bool) {
        self.old_head = self.head;
        self.old_body = self.body;
        self.track.tick();
        self.ticks = self.ticks.saturating_add(1);
        if self.head_steps > 0 {
            // Native head interpolation uses the double overload before narrowing to float.
            let mut delta = f64::from(self.head_target) - f64::from(self.head);
            delta %= 360.;
            if delta >= 180. {
                delta -= 360.;
            } else if delta < -180. {
                delta += 360.;
            }
            self.head = (f64::from(self.head) + delta / f64::from(self.head_steps)) as f32;
            self.head_steps -= 1;
        }
        let dx = self.track.current.position[0] - self.track.previous.position[0];
        let dz = self.track.current.position[2] - self.track.previous.position[2];
        let distance_squared = dx * dx + dz * dz;
        if alive && !passenger {
            self.walk.update(distance_squared.sqrt() as f32, baby);
        } else {
            self.walk = Walk::default();
        }
        if self.kind == BodyKind::Player {
            player::body(self, dx, dz);
            return;
        }
        // BodyRotationControl's native thresholds and getMaxHeadYRot for an unmounted pig.
        const MAX_HEAD: f32 = 75.;
        let rotate = |angle: f32, toward: f32, limit: f32| {
            toward - wrap(toward - angle).clamp(-limit, limit)
        };
        if distance_squared > f64::from(2.5000003E-7_f32) {
            self.body = self.track.current.yaw;
            self.head = rotate(self.head, self.body, MAX_HEAD);
            self.stable_head = self.head;
            self.stable_ticks = 0;
        } else if !carrying {
            if (self.head - self.stable_head).abs() > 15. {
                self.stable_ticks = 0;
                self.stable_head = self.head;
                self.body = rotate(self.body, self.head, MAX_HEAD);
            } else {
                self.stable_ticks = self.stable_ticks.saturating_add(1);
                if self.stable_ticks > 10 {
                    let fraction = ((self.stable_ticks - 10) as f32 / 10.).clamp(0., 1.);
                    self.body = rotate(self.body, self.head, MAX_HEAD * (1. - fraction));
                }
            }
        }
    }
    pub fn sample(&self, fraction: f32) -> Sample {
        let f = fraction.clamp(0., 1.);
        Sample {
            pose: self.track.sample(f),
            age_in_ticks: self.ticks as f32 + f,
            body_yaw: rotation(self.old_body, self.body, f),
            head_yaw: rotation(self.old_head, self.head, f),
            walk_position: (self.walk.position - self.walk.speed * (1. - f)) * self.walk.scale,
            walk_speed: (self.walk.old_speed + (self.walk.speed - self.walk.old_speed) * f).min(1.),
        }
    }
}

#[cfg(test)]
mod tests;
