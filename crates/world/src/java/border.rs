//! Native square-border constraints. Time is supplied by the admission owner,
//! independently of frame cadence; this does not own another simulation loop.
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct Border {
    center: [f64; 2],
    start: f64,
    target: f64,
    duration: Duration,
    absolute_max: i32,
    pub warning_blocks: i32,
    pub warning_time: i32,
}
impl Border {
    pub fn new(
        center: [f64; 2],
        size: f64,
        absolute_max: i32,
        warning_blocks: i32,
        warning_time: i32,
    ) -> Self {
        Self {
            center,
            start: size,
            target: size,
            duration: Duration::ZERO,
            absolute_max,
            warning_blocks,
            warning_time,
        }
    }
    pub fn center(&mut self, center: [f64; 2]) {
        self.center = center;
    }
    pub fn size(&mut self, size: f64) {
        self.start = size;
        self.target = size;
        self.duration = Duration::ZERO;
    }
    pub fn absolute_max(&mut self, value: i32) {
        self.absolute_max = value;
    }
    pub fn lerp(&mut self, from: f64, to: f64, millis: i64) {
        self.start = from;
        self.target = to;
        self.duration = Duration::from_millis(u64::try_from(millis).unwrap_or(0));
    }
    pub fn size_at(self, elapsed: Duration) -> f64 {
        if self.duration.is_zero() || elapsed >= self.duration {
            return self.target;
        }
        self.start
            + (self.target - self.start) * (elapsed.as_secs_f64() / self.duration.as_secs_f64())
    }
    /// WorldBorder.isWithinBounds(BlockPos) admits intersecting block columns,
    /// including fractional centers, with strict comparisons at the outer edge.
    pub fn contains_block(self, position: [i32; 3], elapsed: Duration) -> bool {
        let size = self.size_at(elapsed);
        if size.is_nan() || !self.center.iter().all(|v| v.is_finite()) {
            return false;
        }
        let absolute = f64::from(self.absolute_max);
        [position[0], position[2]]
            .into_iter()
            .zip(self.center)
            .all(|(value, center)| {
                let min = (center - size / 2.).max(-absolute);
                let max = (center + size / 2.).min(absolute);
                f64::from(value) + 1. > min && f64::from(value) < max
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_negative_edges_lerp_and_absolute_cap_are_independent_of_frames() {
        let mut border = Border::new([0., 0.], 16., 100, 5, 15);
        assert!(border.contains_block([-8, 0, -8], Duration::ZERO));
        assert!(!border.contains_block([-9, 0, 0], Duration::ZERO));
        assert!(!border.contains_block([8, 0, 0], Duration::ZERO));
        border.lerp(16., 8., 10_000);
        let half = Duration::from_secs(5);
        assert_eq!(border.size_at(half), 12.);
        assert!(border.contains_block([5, 0, 0], half));
        assert!(!border.contains_block([6, 0, 0], half));
        border.center([0.5, -0.5]);
        assert!(border.contains_block([-6, 0, -6], half));
        assert!(!border.contains_block([-7, 0, 0], half));
        border.absolute_max(2);
        assert!(!border.contains_block([2, 0, 0], half));
        border.lerp(16., 4., -1);
        assert_eq!(border.size_at(Duration::ZERO), 4.);
        border.center([f64::NAN, 0.]);
        assert!(!border.contains_block([0, 0, 0], Duration::ZERO));
        border.center([0., 0.]);
        border.size(f64::INFINITY);
        assert!(border.contains_block([0, 0, 0], Duration::ZERO));
        assert!(!border.contains_block([2, 0, 0], Duration::ZERO));
    }
}
