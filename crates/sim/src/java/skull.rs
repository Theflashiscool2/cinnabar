//! Powered skull decoration clock. Both articulated model periods divide fifty ticks.
#[derive(Clone, Debug, Default)]
pub struct Motion {
    ticks: u8,
    powered: bool,
}
impl Motion {
    pub fn set_powered(&mut self, powered: bool) {
        self.powered = powered;
    }
    pub fn tick(&mut self) {
        if self.powered {
            self.ticks = (self.ticks + 1) % 50;
        }
    }
    pub fn sample(&self, partial: f32) -> f32 {
        f32::from(self.ticks)
            + if self.powered {
                partial.clamp(0., 1.)
            } else {
                0.
            }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn power_stops_without_rewinding_and_the_common_period_is_bounded() {
        let mut m = Motion::default();
        m.tick();
        assert_eq!(m.sample(0.5), 0.);
        m.set_powered(true);
        for _ in 0..7 {
            m.tick();
        }
        assert_eq!(m.sample(0.5), 7.5);
        m.set_powered(false);
        m.tick();
        assert_eq!(m.sample(0.5), 7.);
        m.set_powered(true);
        for _ in 0..50 {
            m.tick();
        }
        assert_eq!(m.sample(0.5), 7.5);
        assert_eq!(Motion::default().sample(0.5), 0.);
    }
}
