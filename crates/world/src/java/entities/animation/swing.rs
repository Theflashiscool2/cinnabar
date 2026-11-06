//! Native LivingEntity.SwingState and humanoid attack poses; visual state never awards hits.
use super::PlayerPosture;
use crate::java::player::trig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    None,
    Whack,
    Stab,
}
impl Kind {
    /// Pinned SwingAnimationType BY_ID uses the native ZERO fallback.
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Whack,
            2 => Self::Stab,
            _ => Self::None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Description {
    offhand: bool,
    kind: Kind,
    duration: i32,
    modified: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub offhand: bool,
    pub kind: Kind,
    pub progress: f32,
}
#[derive(Clone, Debug, Default)]
pub struct State {
    current: Option<Description>,
    ticks: i32,
    previous: f32,
    animation: f32,
}
impl State {
    /// Repeated events cannot restart the first half of an admitted swing.
    pub fn start(&mut self, offhand: bool, kind: Kind, duration: i32, modified: i32) -> bool {
        if self
            .current
            .is_some_and(|s| self.ticks > 0 && self.ticks <= s.modified / 2)
        {
            return false;
        }
        let next = Description {
            offhand,
            kind,
            duration,
            modified,
        };
        if self.current != Some(next) {
            self.previous = 0.;
            self.animation = 0.;
        }
        self.current = Some(next);
        self.ticks = 0;
        true
    }
    pub fn tick(&mut self) {
        self.previous = self.animation;
        if let Some(swing) = self.current {
            if swing.modified > 0 {
                self.animation = ((self.ticks as f32) / (swing.modified as f32)).min(1.);
            }
            let old = self.ticks;
            self.ticks = self.ticks.wrapping_add(1);
            if old > swing.modified {
                self.current = None;
                self.animation = 0.;
            }
        } else {
            self.animation = 0.;
        }
    }
    pub fn sample(&self, fraction: f32) -> Option<Sample> {
        self.current.map(|s| {
            let end = self.animation
                + if self.animation < self.previous {
                    1.
                } else {
                    0.
                };
            Sample {
                offhand: s.offhand,
                kind: s.kind,
                progress: self.previous + fraction.clamp(0., 1.) * (end - self.previous),
            }
        })
    }
    pub fn turning_body(&self) -> bool {
        self.animation > 0.
    }
}

/// LivingEntity.getModifiedSwingDuration, shared by local and remote visual admission.
pub fn modified_duration(
    effects: &std::collections::BTreeMap<String, crate::java::dynamics::Effect>,
    duration: i32,
) -> i32 {
    let speed = ["minecraft:haste", "minecraft:conduit_power"]
        .into_iter()
        .filter_map(|name| effects.get(name).map(|e| e.amplifier))
        .max();
    if let Some(speed) = speed {
        return duration.wrapping_sub(1 + i32::from(speed));
    }
    if let Some(fatigue) = effects.get("minecraft:mining_fatigue") {
        return duration.wrapping_add((1 + i32::from(fatigue.amplifier)) * 2);
    }
    duration
}

impl PlayerPosture {
    /// Runs between native idle item arm poses and crouch/bob adjustments.
    pub(super) fn attack(&mut self, swing: Sample, left_handed: bool) {
        let f = swing.progress;
        if f <= 0. {
            return;
        }
        let sin = |x| trig(x)[0] as f32;
        let cos = |x| trig(x)[1] as f32;
        let left = swing.offhand != left_handed;
        let arm = if left { 3 } else { 2 };
        let yaw = sin(f.sqrt() * std::f32::consts::TAU) * 0.2 * if left { -1. } else { 1. };
        self.rotations[1][1] = yaw;
        self.offsets[2][2] = sin(yaw) * 5.;
        self.offsets[2][0] = -cos(yaw) * 5.;
        self.offsets[3][2] = -sin(yaw) * 5.;
        self.offsets[3][0] = cos(yaw) * 5.;
        self.rotations[2][1] += yaw;
        self.rotations[3][1] += yaw;
        self.rotations[3][0] += yaw;
        match swing.kind {
            Kind::None => {}
            Kind::Whack => {
                let t = 1. - f;
                let t = 1. - t * t * t * t;
                let reach = sin(t * std::f32::consts::PI);
                let head = sin(f * std::f32::consts::PI) * -(self.rotations[0][0] - 0.7) * 0.75;
                self.rotations[arm][0] -= reach * 1.2 + head;
                self.rotations[arm][1] += yaw * 2.;
                self.rotations[arm][2] += sin(f * std::f32::consts::PI) * -0.4;
            }
            Kind::Stab => {
                self.rotations[2][1] -= yaw;
                self.rotations[3][1] -= yaw;
                self.rotations[3][0] -= yaw;
                let progress = |lo: f32, hi: f32| ((f - lo) / (hi - lo)).clamp(0., 1.);
                let a = -(cos(std::f32::consts::PI * progress(0., 0.05)) - 1.) / 2.;
                let b = progress(0.05, 0.2).powi(2);
                let x = progress(0.4, 1.);
                let c = if x == 0. || x == 1. {
                    x
                } else if x < 0.5 {
                    2f64.powf(f64::from(20. * x - 10.)) as f32 / 2.
                } else {
                    (2. - 2f64.powf(f64::from(-20. * x + 10.)) as f32) / 2.
                };
                self.rotations[arm][0] +=
                    (90. * a - 120. * b + 30. * c) * (std::f32::consts::PI / 180.);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_humanoid_reference_vectors_cover_every_animation_and_both_hands() {
        // Independent verified26.3 HumanoidModel.setupAnim output, including the NONE twist.
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("swing_vectors.json")).unwrap();
        assert_eq!(vectors.as_array().unwrap().len(), 30);
        for v in vectors.as_array().unwrap() {
            let sample = super::super::Sample {
                pose: Default::default(),
                body_yaw: 0.,
                head_yaw: 0.,
                walk_position: 0.,
                walk_speed: 0.,
                age_in_ticks: 0.,
            };
            let posture = sample.player_posture_with_swing(
                false,
                false,
                false,
                Some(Sample {
                    kind: Kind::from_id(v["kind"].as_i64().unwrap() as i32),
                    offhand: v["offhand"].as_bool().unwrap(),
                    progress: v["progress"].as_f64().unwrap() as f32,
                }),
                false,
            );
            for i in 0..6 {
                for axis in 0..3 {
                    let rotation = v["parts"][i][axis].as_f64().unwrap() as f32;
                    let offset = v["parts"][i][axis + 3].as_f64().unwrap() as f32;
                    assert!(
                        (posture.rotations[i][axis] - rotation).abs() < 0.00002,
                        "rotation {v} part{i} axis{axis}: {} != {rotation}",
                        posture.rotations[i][axis]
                    );
                    assert!(
                        (posture.offsets[i][axis] - offset).abs() < 0.00002,
                        "offset {v} part{i} axis{axis}: {} != {offset}",
                        posture.offsets[i][axis]
                    );
                }
            }
        }
    }
    #[test]
    fn native_speed_precedence_and_fatigue_duration() {
        use crate::java::dynamics::Effect;
        let mut effects = std::collections::BTreeMap::new();
        effects.insert(
            "minecraft:mining_fatigue".into(),
            Effect {
                amplifier: 2,
                duration: -1,
                flags: 0,
            },
        );
        assert_eq!(modified_duration(&effects, 6), 12);
        effects.insert(
            "minecraft:haste".into(),
            Effect {
                amplifier: 0,
                duration: 20,
                flags: 0,
            },
        );
        assert_eq!(modified_duration(&effects, 6), 5);
        effects.insert(
            "minecraft:conduit_power".into(),
            Effect {
                amplifier: 3,
                duration: 20,
                flags: 0,
            },
        );
        assert_eq!(modified_duration(&effects, 6), 2);
        effects.get_mut("minecraft:haste").unwrap().amplifier = 10;
        assert_eq!(modified_duration(&effects, 6), -5);
    }
    #[test]
    fn native_tick_restart_window_and_nonpositive_durations() {
        let mut state = State::default();
        assert!(state.start(false, Kind::Whack, 6, 6));
        for expected in [
            0.,
            0.,
            0.083333336,
            0.25,
            0.4166667,
            0.5833334,
            0.75,
            0.9166666,
        ] {
            assert!((state.sample(0.5).unwrap().progress - expected).abs() < 0.000001);
            state.tick();
        }
        assert!(state.sample(0.5).is_none());
        let mut state = State::default();
        for expected in [true, false, false, false, true, false, false, false] {
            assert_eq!(state.start(false, Kind::Whack, 6, 6), expected);
            state.tick();
        }
        for (duration, ticks) in [(-2, 1), (0, 2)] {
            let mut state = State::default();
            state.start(false, Kind::Whack, duration, duration);
            for _ in 0..ticks {
                assert_eq!(state.sample(1.).unwrap().progress, 0.);
                state.tick();
            }
            assert!(state.sample(1.).is_none());
        }
        assert_eq!(Kind::from_id(-1), Kind::None);
        assert_eq!(Kind::from_id(3), Kind::None);
    }
}
