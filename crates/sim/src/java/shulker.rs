//! Java ShulkerBoxBlockEntity's tick progress and selection bounds.
use assets::registry::java::physics::Bounds;

#[derive(Clone, Debug, Default)]
pub struct Motion {
    old: f32,
    current: f32,
    opening: bool,
}
impl Motion {
    /// Counts above one retain the previous animation direction in vanilla.
    pub fn event(&mut self, transition: Option<u8>) {
        match transition {
            Some(0) => self.opening = false,
            Some(1) => self.opening = true,
            _ => {}
        }
    }
    pub fn tick(&mut self) {
        self.old = self.current;
        self.current = (self.current + if self.opening { 0.1 } else { -0.1 }).clamp(0., 1.);
    }
    pub fn sample(&self, partial: f32) -> f32 {
        self.old + (self.current - self.old) * partial.clamp(0., 1.)
    }
    /// Picking uses getProgress(1), not the render interpolation or shell's 0.9995 scale.
    pub fn outline(&self, facing: u8) -> Option<Bounds> {
        let (axis, positive) = match facing {
            0 => (1, false),
            1 => (1, true),
            2 => (2, false),
            3 => (2, true),
            4 => (0, false),
            5 => (0, true),
            _ => return None,
        };
        let mut bounds = Bounds {
            min: [0.; 3],
            max: [1.; 3],
        };
        let extension = f64::from(0.5 * self.current);
        if positive {
            bounds.max[axis] += extension;
        } else {
            bounds.min[axis] -= extension;
        }
        Some(bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_transitions_and_native_ten_tick_bounds() {
        let mut motion = Motion::default();
        motion.event(Some(2));
        motion.tick();
        assert_eq!(motion.sample(1.), 0.);
        motion.event(Some(1));
        for _ in 0..5 {
            motion.tick();
        }
        assert!((motion.sample(1.) - 0.5).abs() < 1e-6);
        assert!((motion.sample(0.5) - 0.45).abs() < 1e-6);
        motion.event(Some(3));
        for _ in 0..5 {
            motion.tick();
        }
        for (facing, axis, positive) in [
            (0, 1, false),
            (1, 1, true),
            (2, 2, false),
            (3, 2, true),
            (4, 0, false),
            (5, 0, true),
        ] {
            let b = motion.outline(facing).unwrap();
            let mut min = [0.; 3];
            let mut max = [1.; 3];
            if positive {
                max[axis] = 1.5;
            } else {
                min[axis] = -0.5;
            }
            assert_eq!(b.min, min);
            assert_eq!(b.max, max);
        }
        motion.event(Some(0));
        for _ in 0..10 {
            motion.tick();
        }
        assert_eq!(motion.sample(1.), 0.);
        assert_eq!(motion.outline(1).unwrap().max, [1.; 3]);
        assert!(motion.outline(6).is_none());
    }
}
