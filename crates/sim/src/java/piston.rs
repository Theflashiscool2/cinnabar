//! Pinned Java piston presentation clock; authoritative blocks remain owned by the world.
//! Reference: PistonMovingBlockEntity.tick/getProgress/getExtendedProgress in Java 26.3.

#[derive(Clone, Debug)]
pub struct Motion {
    previous: f32,
    current: f32,
    completed_ticks: u8,
    retired: bool,
}

impl Motion {
    /// Network snapshots serialize the previous progress; loading starts both samples there.
    pub fn new(progress: f32) -> Result<Self, &'static str> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err("invalid Java piston progress");
        }
        Ok(Self {
            previous: progress,
            current: progress,
            completed_ticks: 0,
            retired: false,
        })
    }

    /// One native 20 Hz tick. Completed client entities get five grace ticks before retirement.
    /// This never writes a final block or predicts acceptance of a server transaction.
    pub fn tick(&mut self) {
        if self.retired {
            return;
        }
        self.previous = self.current;
        if self.previous >= 1.0 {
            if self.completed_ticks < 5 {
                self.completed_ticks += 1;
            } else {
                self.retired = true;
            }
        } else {
            self.current = (self.current + 0.5).min(1.0);
        }
    }

    pub fn retired(&self) -> bool {
        self.retired
    }

    /// Render interpolation does not advance simulation or the retirement grace period.
    pub fn progress(&self, partial: f32) -> f32 {
        self.previous + (self.current - self.previous) * partial.clamp(0.0, 1.0)
    }

    /// Offset from the moving-block destination, using native down/up/north/south/west/east.
    pub fn offset(&self, facing: u8, extending: bool, partial: f32) -> Option<[f32; 3]> {
        let step = world::java::piston::step(facing)?;
        let p = self.progress(partial);
        let distance = if extending { p - 1.0 } else { 1.0 - p };
        Some(step.map(|n| n as f32 * distance))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_native_ticks_then_five_grace_ticks_do_not_depend_on_render_frequency() {
        let mut motion = Motion::new(0.0).unwrap();
        motion.tick();
        assert_eq!(motion.progress(0.5), 0.25);
        for _ in 0..100 {
            assert_eq!(motion.offset(5, true, 0.5), Some([-0.75, 0.0, 0.0]));
        }
        motion.tick();
        assert_eq!(motion.progress(0.5), 0.75);
        for _ in 0..5 {
            motion.tick();
            assert!(!motion.retired());
            assert_eq!(motion.progress(0.0), 1.0);
        }
        motion.tick();
        assert!(motion.retired());
        motion.tick();
        assert_eq!(motion.progress(1.0), 1.0);
    }

    #[test]
    fn loaded_progress_resumes_without_a_jump_or_invalid_axis() {
        let mut motion = Motion::new(0.5).unwrap();
        assert_eq!(motion.offset(2, false, 0.5), Some([0.0, 0.0, -0.5]));
        motion.tick();
        assert_eq!(motion.offset(2, false, 0.5), Some([0.0, 0.0, -0.25]));
        assert_eq!(motion.offset(6, false, 0.0), None);
        for invalid in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
            assert!(Motion::new(invalid).is_err());
        }
    }

    #[test]
    fn offsets_match_the_pinned_original_client_probe() {
        let Ok(path) = std::env::var("CINNABAR_JAVA_PISTON_MOTION") else {
            eprintln!("missing fixture: CINNABAR_JAVA_PISTON_MOTION (InspectMovingPiston output)");
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let mut checked = 0;
        for line in text.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 8 || fields[0].parse::<u8>().is_err() {
                continue;
            }
            let facing = fields[0].parse().unwrap();
            let extending = fields[1].parse().unwrap();
            let start = fields[2].parse().unwrap();
            let partial = fields[3].parse().unwrap();
            let mut motion = Motion::new(start).unwrap();
            motion.tick();
            let expected: Vec<f32> = fields[4..].iter().map(|n| n.parse().unwrap()).collect();
            assert_eq!(motion.progress(partial), expected[0], "{line}");
            assert_eq!(
                motion.offset(facing, extending, partial).unwrap(),
                expected[1..],
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 144);
    }
}
