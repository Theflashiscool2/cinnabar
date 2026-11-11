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
