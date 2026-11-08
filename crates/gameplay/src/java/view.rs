//! Read-only camera history of completed native movement ticks. Never feeds physics or packets.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub feet: [f64; 3],
    pub eye_height: f64,
}

#[derive(Default)]
pub(super) struct History(Option<(Sample, Sample)>);
impl History {
    pub fn reset(&mut self, sample: Sample) {
        self.0 = Some((sample, sample));
    }

    pub fn tick(&mut self, before: Sample, after: Sample) {
        let old = self.0.map_or(before, |(_, current)| current);
        // Camera.tick in the pinned client approaches the entity's eye height by half
        // per simulation tick; rendering then interpolates those two camera heights.
        let next = Sample {
            feet: after.feet,
            eye_height: old.eye_height + (after.eye_height - old.eye_height) * 0.5,
        };
        self.0 = Some((
            Sample {
                feet: before.feet,
                ..old
            },
            next,
        ));
    }

    pub fn sample(&self, fraction: f64) -> Option<Sample> {
        let (old, current) = self.0?;
        let t = fraction.clamp(0., 1.);
        Some(Sample {
            feet: std::array::from_fn(|i| old.feet[i] + (current.feet[i] - old.feet[i]) * t),
            eye_height: old.eye_height + (current.eye_height - old.eye_height) * t,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crouch_and_travel_interpolate_without_extrapolation_and_reset_snaps() {
        let standing = Sample {
            feet: [-1., 4., 15.9],
            eye_height: 1.62,
        };
        let crouching = Sample {
            feet: [-0.8, 4., 16.1],
            eye_height: 1.27,
        };
        let mut history = History::default();
        history.reset(standing);
        history.tick(standing, crouching);
        let half = history.sample(0.5).unwrap();
        assert!((half.feet[0] + 0.9).abs() < 1e-10);
        assert!((half.eye_height - 1.5325).abs() < 1e-10);
        assert_eq!(history.sample(20.).unwrap().feet, crouching.feet);
        history.reset(standing);
        assert_eq!(history.sample(0.5).unwrap().feet, standing.feet);
        assert_eq!(history.sample(0.5).unwrap().eye_height, standing.eye_height);
    }
}
