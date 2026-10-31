//! Java gateway age and authoritative cooldown events, independent of rendering rate.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    age: i64,
    authority_revision: Option<u64>,
    cooldown: u8,
    event_revision: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub scale: f32,
    pub height: i32,
    pub spawning: bool,
}
impl Motion {
    /// Repeated extraction of unchanged entity data must not rewind local animation.
    pub fn synchronize(&mut self, age: i64, revision: u64) {
        if self.authority_revision != Some(revision) {
            self.authority_revision = Some(revision);
            self.age = age;
        }
    }
    pub fn event(&mut self, revision: u64) {
        if self.event_revision != Some(revision) {
            self.event_revision = Some(revision);
            self.cooldown = 40;
        }
    }
    pub fn tick(&mut self) {
        self.age = self.age.saturating_add(1);
        self.cooldown = self.cooldown.saturating_sub(1);
    }
    pub fn sample(&self, partial: f32, max_y: i32) -> Option<Beam> {
        let spawning = self.age < 200;
        if !spawning && self.cooldown == 0 {
            return None;
        }
        let partial = partial.clamp(0., 1.);
        let progress = if spawning {
            (self.age as f32 + partial) / 200.
        } else {
            1. - (self.cooldown as f32 - partial) / 40.
        };
        let scale = (progress.clamp(0., 1.) * std::f32::consts::PI).sin();
        let height = (scale as f64 * if spawning { max_y as f64 } else { 50. }).floor() as i32;
        Some(Beam {
            scale,
            height,
            spawning,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_duration_authority_and_event_order() {
        let mut m = Motion::default();
        m.synchronize(0, 1);
        for _ in 0..100 {
            m.tick();
        }
        m.synchronize(0, 1);
        assert_eq!(
            m.sample(0., 320),
            Some(Beam {
                scale: 1.,
                height: 320,
                spawning: true
            })
        );
        m.synchronize(0, 2);
        assert_eq!(m.sample(0., 320).unwrap().height, 0);
        for _ in 0..100 {
            m.tick();
        }
        m.event(1); // spawning takes precedence
        assert!(m.sample(0., 256).unwrap().spawning);
        for _ in 0..100 {
            m.tick();
        }
        assert_eq!(m.sample(0., 320), None);
        m.event(2);
        for _ in 0..20 {
            m.tick();
        }
        m.event(2);
        assert_eq!(
            m.sample(0., 320),
            Some(Beam {
                scale: 1.,
                height: 50,
                spawning: false
            })
        );
        for _ in 0..20 {
            m.tick();
        }
        assert_eq!(m.sample(0., 320), None);
        m.event(3);
        assert_eq!(m.sample(0., 320).unwrap().height, 0);
        m.synchronize(100, 3);
        assert_eq!(m.sample(0., 128).unwrap().height, 128);
        m.synchronize(i64::MAX, 4);
        m.tick();
        assert!(!m.sample(0., 320).unwrap().spawning);
    }
}
