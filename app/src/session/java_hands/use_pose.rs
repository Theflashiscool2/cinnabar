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
/// Active use owns the hand pose. Only the current swing hand enters its native animation branch.
pub(super) fn attack(
    swing: Option<world::java::entities::animation::swing::Sample>,
    offhand: bool,
    using: bool,
) -> Mat4 {
    use world::java::entities::animation::swing::Kind;
    swing
        .filter(|s| !using && s.offhand == offhand && s.kind == Kind::Whack)
        .map_or(Mat4::IDENTITY, |s| whack(s.progress, !offhand))
}
/// Native swingArm after the ordinary equip translation. Geometry remains shared/immutable.
pub(super) fn whack(progress: f32, right: bool) -> Mat4 {
    use std::f32::consts::{PI, TAU};
    let p = progress.clamp(0., 1.);
    let sin = |v| world::java::player::trig(v)[0] as f32;
    let root = p.sqrt();
    let sign = if right { 1. } else { -1. };
    Mat4::from_translation(Vec3::new(
        sign * -0.4 * sin(root * PI),
        0.2 * sin(root * TAU),
        -0.2 * sin(p * PI),
    )) * Mat4::from_rotation_y((sign * (45. - 20. * sin(p * p * PI))).to_radians())
        * Mat4::from_rotation_z((sign * -20. * sin(root * PI)).to_radians())
        * Mat4::from_rotation_x((-80. * sin(root * PI)).to_radians())
        * Mat4::from_rotation_y((sign * -45.).to_radians())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whack_matches_verified_native26_3_matrices_for_both_arms() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("swing_vectors.json")).unwrap();
        assert_eq!(vectors.as_array().unwrap().len(), 12);
        for v in vectors.as_array().unwrap() {
            let actual = whack(
                v["phase"].as_f64().unwrap() as f32,
                v["right"].as_bool().unwrap(),
            )
            .to_cols_array();
            for (i, value) in actual.into_iter().enumerate() {
                assert!(
                    (value - v["matrix"][i].as_f64().unwrap() as f32).abs() < 0.0002,
                    "{v} entry{i}: {value}"
                );
            }
        }
    }
    #[test]
    fn native_active_use_and_other_hand_do_not_inherit_attack_transforms() {
        use world::java::entities::animation::swing::{Kind, Sample};
        let swing = Some(Sample {
            offhand: false,
            kind: Kind::Whack,
            progress: 0.5,
        });
        assert_eq!(attack(swing, false, false), whack(0.5, true));
        assert_eq!(attack(swing, true, false), Mat4::IDENTITY);
        assert_eq!(attack(swing, false, true), Mat4::IDENTITY);
        assert_eq!(attack(None, false, false), Mat4::IDENTITY);
        assert_eq!(
            attack(
                Some(Sample {
                    kind: Kind::None,
                    ..swing.unwrap()
                }),
                false,
                false
            ),
            Mat4::IDENTITY
        );
    }
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
