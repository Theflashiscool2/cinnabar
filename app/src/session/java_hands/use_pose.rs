//! Native26.3 applyEatTransform, before the ordinary equip translation. This is
//! presentation only; it neither ticks nor finishes the authoritative use action.
use bevy::math::{Mat4, Vec3};
pub(super) fn eat(elapsed: u32, duration: u32, partial: f32, right: bool) -> Mat4 {
    if duration == 0 {
        return Mat4::IDENTITY;
    }
    let remaining = duration.saturating_sub(elapsed) as f32 - partial + 1.;
    let ratio = remaining / duration as f32;
    let bob = if ratio < 0.8 {
        ((remaining / 4. * std::f32::consts::PI) as f64).cos().abs() as f32 * 0.1
    } else {
        0.
    };
    let approach = 1. - (ratio as f64).powf(27.) as f32;
    let sign = if right { 1. } else { -1. };
    Mat4::from_translation(Vec3::new(
        approach * 0.6 * sign,
        bob + approach * (-0.5),
        0.,
    )) * Mat4::from_rotation_y((sign * approach * 90.).to_radians())
        * Mat4::from_rotation_x((approach * 10.).to_radians())
        * Mat4::from_rotation_z((sign * approach * 30.).to_radians())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eating_uses_target_remaining_ticks_and_mirrors_hands_without_mutating_geometry() {
        let right = eat(17, 32, 0., true);
        let left = eat(17, 32, 0., false);
        let r = right.transform_point3(Vec3::ZERO);
        let l = left.transform_point3(Vec3::ZERO);
        assert!((r.x - 0.6).abs() < 1e-6);
        assert!((r.y + 0.4).abs() < 1e-6);
        assert!((r.x + l.x).abs() < 1e-6);
        assert_eq!(r.y, l.y);
        assert_eq!(eat(0, 0, 0., true), Mat4::IDENTITY);
    }
}
