//! LocalPlayer.aiStep's forward-tap latch, advanced only by completed native ticks.
//! 26.3 Options.sprintWindow defaults to seven; ClientInput.hasForwardImpulse uses
//! 1e-5 (not the Bedrock analog threshold). This owns no separate clock or packets.
use sim::java::travel::Input;
const WINDOW: u8 = 7;
const FORWARD: f32 = 1.0e-5;
#[derive(Clone, Copy, Default)]
pub(super) struct Trigger {
    remaining: u8,
    forward: bool,
    crouch: bool,
    released: bool,
}
impl Trigger {
    pub(super) fn release(&mut self) {
        *self = Self {
            released: true,
            ..Default::default()
        };
    }
    pub(super) fn select(
        &mut self,
        input: Input,
        was: bool,
        possible: bool,
        can_start: bool,
        slow: bool,
        backward: bool,
    ) -> bool {
        self.remaining = self.remaining.saturating_sub(1);
        if self.crouch || slow || backward {
            self.remaining = 0;
        }
        let forward = input.forward > FORWARD;
        let was = was && !self.released;
        self.released = false;
        let mut tapped = false;
        if !was && forward && possible && can_start && !slow && !self.forward {
            if self.remaining > 0 {
                tapped = true;
            } else {
                self.remaining = WINDOW;
            }
        }
        self.forward = forward;
        self.crouch = input.crouch;
        possible && forward && !slow && (was || (can_start && (input.sprint || tapped)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tick(t: &mut Trigger, forward: f32, was: bool) -> bool {
        t.select(
            Input {
                forward,
                ..Default::default()
            },
            was,
            true,
            true,
            false,
            false,
        )
    }
    #[test]
    fn two_forward_edges_start_and_hold_without_a_sprint_key() {
        let mut t = Trigger::default();
        assert!(!tick(&mut t, 1., false));
        assert!(!tick(&mut t, 0., false));
        assert!(tick(&mut t, 1., false));
        assert!(tick(&mut t, 1., true));
        assert!(!tick(&mut t, 0., true));
        // Native setSprinting does not clear the remaining tap timer.
        assert!(tick(&mut t, 1., false));
    }
    #[test]
    fn seven_tick_window_expires_on_completed_ticks_not_frames() {
        for idle in [5, 6] {
            let mut t = Trigger::default();
            assert!(!tick(&mut t, 1., false));
            for _ in 0..idle {
                assert!(!tick(&mut t, 0., false));
            }
            assert_eq!(tick(&mut t, 1., false), idle == 5);
        }
    }
    #[test]
    fn release_crouch_backward_item_use_and_denied_admission_cannot_start_sprint() {
        let input = Input {
            forward: 1.,
            ..Default::default()
        };
        for (possible, start, slow, backward) in [
            (false, true, false, false),
            (true, false, false, false),
            (true, true, true, false),
            (true, true, false, true),
        ] {
            let mut t = Trigger::default();
            tick(&mut t, 1., false);
            tick(&mut t, 0., false);
            assert!(!t.select(input, false, possible, start, slow, backward));
        }
        let mut t = Trigger::default();
        tick(&mut t, 1., false);
        t.release();
        assert!(!tick(&mut t, 1., true));
        let mut t = Trigger::default();
        tick(&mut t, 1., false);
        t.select(
            Input {
                crouch: true,
                ..Default::default()
            },
            false,
            true,
            false,
            false,
            false,
        );
        assert!(!tick(&mut t, 1., false));
    }
    #[test]
    fn sprint_key_can_start_without_double_tap_and_release_keeps_native_sprint() {
        let mut t = Trigger::default();
        assert!(t.select(
            Input {
                forward: 1.,
                sprint: true,
                ..Default::default()
            },
            false,
            true,
            true,
            false,
            false
        ));
        assert!(tick(&mut t, 1., true));
        assert!(!t.select(
            Input {
                forward: 1.,
                ..Default::default()
            },
            true,
            false,
            true,
            false,
            false
        ));
        // Cloning for a blocked/failed tick leaves the real trigger unconsumed.
        let mut t = Trigger::default();
        tick(&mut t, 1., false);
        tick(&mut t, 0., false);
        let mut failed = t;
        assert!(tick(&mut failed, 1., false));
        assert!(tick(&mut t, 1., false));
    }
}
