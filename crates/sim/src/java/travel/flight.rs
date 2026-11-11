//! Native LocalPlayer vertical controls and Player's active-flight travel wrapper.
use super::Input;

pub(super) fn lift(y: &mut f64, input: Input, speed: f32) {
    let direction = i32::from(input.jump) - i32::from(input.crouch);
    if direction != 0 {
        *y += f64::from(direction as f32 * speed * 3.);
    }
}
pub(super) fn speed(speed: f32, sprinting: bool) -> f32 {
    if sprinting { speed * 2. } else { speed }
}
pub(super) fn damp(y: f64) -> f64 {
    y * 0.6
}

const TOGGLE_WINDOW: u8 = 7;

/// Official 26.3 LocalPlayer.aiStep offsets 501..654 check the physical Jump edge;
/// Player.aiStep offsets 0..17 then decrement its latch. The caller is unmounted and
/// supplies manual Jump (the client has no terrain auto-jump input producer).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Trigger {
    previous_jump: bool,
    remaining: u8,
}
impl Trigger {
    pub(super) fn tick(
        &mut self,
        jump: bool,
        may_fly: bool,
        spectator: bool,
        flying: bool,
        swimming: bool,
    ) -> Option<bool> {
        let mut changed = None;
        if may_fly {
            if spectator {
                if !flying {
                    changed = Some(true);
                }
            } else if jump && !self.previous_jump {
                if self.remaining == 0 {
                    self.remaining = TOGGLE_WINDOW;
                } else if !swimming {
                    changed = Some(!flying);
                    self.remaining = 0;
                }
            }
        }
        self.previous_jump = jump;
        self.remaining = self.remaining.saturating_sub(1);
        changed
    }
}
