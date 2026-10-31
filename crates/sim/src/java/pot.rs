//! Bounded presentation clock for Java decorated-pot block events.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    revision: Option<u64>,
    style: u8,
    age: u8,
}
impl Motion {
    pub fn event(&mut self, revision: u64, style: u8) {
        if style < 2 && self.revision != Some(revision) {
            self.revision = Some(revision);
            self.style = style;
            self.age = 0;
        }
    }
    pub fn tick(&mut self) {
        self.age = self.age.saturating_add(1);
    }
    pub fn sample(&self, partial: f32) -> Option<(u8, f32)> {
        let duration = if self.style == 0 { 7. } else { 10. };
        let elapsed = f32::from(self.age) + partial.clamp(0., 1.);
        (self.revision.is_some() && elapsed <= duration).then_some((self.style, elapsed / duration))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn events_restart_once_and_finish_at_native_durations() {
        let mut m = Motion::default();
        assert_eq!(m.sample(0.), None);
        m.event(1, 0);
        for _ in 0..3 {
            m.tick();
        }
        m.event(1, 0);
        assert_eq!(m.sample(0.5), Some((0, 0.5)));
        for _ in 0..5 {
            m.tick();
        }
        assert_eq!(m.sample(0.), None);
        m.event(2, 1);
        for _ in 0..5 {
            m.tick();
        }
        assert_eq!(m.sample(0.), Some((1, 0.5)));
        m.event(3, 9);
        assert_eq!(m.sample(0.), Some((1, 0.5)));
        for _ in 0..300 {
            m.tick();
        }
        assert_eq!(m.sample(0.), None);
        m.event(4, 0);
        assert_eq!(m.sample(0.), Some((0, 0.)));
    }
}
