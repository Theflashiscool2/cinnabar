//! Java conduit presentation derived from authoritative water and block tags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub active: bool,
    pub hunting: bool,
    pub frame_blocks: u8,
}
/// Missing terrain is deliberately inactive until the next bounded shape refresh.
pub fn shape(mut sample: impl FnMut([i32; 3]) -> Option<(bool, bool)>) -> Shape {
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                if !sample([x, y, z]).is_some_and(|s| s.0) {
                    return Shape::default();
                }
            }
        }
    }
    let mut count = 0;
    for x in -2i32..=2 {
        for y in -2i32..=2 {
            for z in -2i32..=2 {
                if (x == 0 && (y.abs() == 2 || z.abs() == 2))
                    || (y == 0 && (x.abs() == 2 || z.abs() == 2))
                    || (z == 0 && (x.abs() == 2 || y.abs() == 2))
                {
                    count += u8::from(sample([x, y, z]).is_some_and(|s| s.1));
                }
            }
        }
    }
    Shape {
        active: count >= 16,
        hunting: count >= 42,
        frame_blocks: count,
    }
}
#[derive(Clone, Debug, Default)]
pub struct Motion {
    pub shape: Shape,
    ticks: u64,
    active_ticks: f64,
}
impl Motion {
    pub fn tick(&mut self, sample: impl FnMut([i32; 3]) -> Option<(bool, bool)>) {
        self.ticks = self.ticks.saturating_add(1);
        if self.ticks.is_multiple_of(40) {
            self.shape = shape(sample);
        }
        if self.shape.active {
            self.active_ticks += 1.;
        }
    }
    pub fn sample(&self, partial: f32) -> (f32, f32) {
        let partial = f64::from(partial.clamp(0., 1.));
        (
            (self.ticks as f64 + partial) as f32,
            ((self.active_ticks + if self.shape.active { partial } else { 0. }) * -0.0375) as f32,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn water_frame_thresholds_missing_neighbors_and_removal() {
        let full = |p: [i32; 3]| Some((p.iter().all(|n| n.abs() <= 1), true));
        assert_eq!(
            shape(full),
            Shape {
                active: true,
                hunting: true,
                frame_blocks: 42
            }
        );
        let ring = |p: [i32; 3]| Some((p.iter().all(|n| n.abs() <= 1), p[1] == 0));
        assert_eq!(
            shape(ring),
            Shape {
                active: true,
                hunting: false,
                frame_blocks: 16
            }
        );
        assert_eq!(
            shape(|p| if p == [-2, 0, 0] { None } else { ring(p) }).frame_blocks,
            15
        );
        assert!(!shape(|p| if p == [1, 1, 1] { None } else { full(p) }).active);
        assert!(!shape(|p| Some((p != [0, 0, 0], true))).active);
        let mut m = Motion::default();
        for _ in 0..39 {
            m.tick(full)
        }
        assert!(!m.shape.active);
        m.tick(full);
        assert!(m.shape.active);
        assert!((m.sample(0.).1 + 0.0375).abs() < 1e-6);
        for _ in 0..40 {
            m.tick(|_| None)
        }
        assert!(!m.shape.active);
        let angle = m.sample(0.).1;
        for _ in 0..40 {
            m.tick(|_| None)
        }
        assert_eq!(m.sample(1.).1, angle);
        for _ in 0..40 {
            m.tick(ring)
        }
        assert!(m.shape.active);
        assert!(!m.shape.hunting);
    }
}
