//! Native adult ItemInHandLayer transform, after the sampled model arm transform.
use crate::RenderBoneTransform;
use glam::{Mat4, Quat, Vec3};
fn matrix(b: &RenderBoneTransform) -> Option<Mat4> {
    if !b.is_finite() {
        return None;
    }
    let q = Quat::from_vec4(glam::Vec4::from_array(b.rotation).try_normalize()?);
    Some(Mat4::from_scale_rotation_translation(
        Vec3::from_array([b.axis_scale[0], b.axis_scale[1], b.axis_scale[2]])
            * b.translation_scale[3],
        q,
        Vec3::from_array([
            b.translation_scale[0],
            b.translation_scale[1],
            b.translation_scale[2],
        ]),
    ))
}
pub fn held_bone(
    hand: RenderBoneTransform,
    root: RenderBoneTransform,
    left: bool,
    slim: bool,
) -> Option<RenderBoneTransform> {
    let mut m = matrix(&hand)?;
    let sign = if left { -1. } else { 1. };
    // PlayerModel changes arm.x BEFORE translateAndRotate for slim arms. This is
    // a parent-frame offset, not an item-local translation after arm rotation.
    if slim {
        m.w_axis +=
            (matrix(&root)?.transform_vector3(Vec3::new(sign * 0.5 / 16., 0., 0.))).extend(0.);
    }
    m = m
        * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
        * Mat4::from_rotation_y(std::f32::consts::PI)
        * Mat4::from_translation(Vec3::new(sign / 16., 2. / 16., -10. / 16.));
    let (scale, q, p) = m.to_scale_rotation_translation();
    let rebuilt = Mat4::from_scale_rotation_translation(scale, q, p);
    if m.to_cols_array()
        .iter()
        .zip(rebuilt.to_cols_array())
        .any(|(a, b)| !a.is_finite() || (*a - b).abs() > 1e-5)
    {
        return None;
    }
    let b = RenderBoneTransform {
        rotation: q.to_array(),
        translation_scale: [p.x, p.y, p.z, 1.],
        axis_scale: [scale.x, scale.y, scale.z, 1.],
    };
    b.is_finite().then_some(b)
}
/// Pinned CustomHeadLayer.DEFAULT: raw skull geometry follows the sampled head at 19/16 scale.
pub fn skull_bone(head: RenderBoneTransform) -> Option<RenderBoneTransform> {
    matrix(&head)?;
    let mut bone = head;
    for scale in &mut bone.axis_scale[..3] {
        *scale *= 1.1875;
    }
    bone.is_finite().then_some(bone)
}
pub fn posed_bounds(
    bounds: assets::SkinGeometryBounds,
    bone: &RenderBoneTransform,
) -> Option<assets::SkinGeometryBounds> {
    let m = matrix(bone)?;
    let c = Vec3::from_array(bounds.center);
    let h = Vec3::from_array(bounds.half_extents);
    let center = m.transform_point3(c);
    let half = m.x_axis.truncate().abs() * h.x
        + m.y_axis.truncate().abs() * h.y
        + m.z_axis.truncate().abs() * h.z;
    (center.is_finite() && half.is_finite()).then_some(assets::SkinGeometryBounds {
        center: center.to_array(),
        half_extents: half.to_array(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn identity() -> RenderBoneTransform {
        RenderBoneTransform {
            rotation: [0., 0., 0., 1.],
            translation_scale: [0., 0., 0., 1.],
            axis_scale: [1.; 4],
        }
    }
    #[test]
    fn worn_skull_follows_head_rotation_without_item_translation() {
        // 26.3 CustomHeadLayer.submit: head transform, then uniform 1.1875 scale.
        let mut head = identity();
        head.rotation = Quat::from_rotation_y(0.7).to_array();
        head.translation_scale = [1., 2., 3., 1.];
        let skull = skull_bone(head).unwrap();
        assert_eq!(skull.rotation, head.rotation);
        assert_eq!(skull.translation_scale, head.translation_scale);
        assert_eq!(skull.axis_scale, [1.1875, 1.1875, 1.1875, 1.]);
        let bound = posed_bounds(
            assets::SkinGeometryBounds {
                center: [0., -0.25, 0.],
                half_extents: [0.25; 3],
            },
            &skull,
        )
        .unwrap();
        assert!((bound.center[1] - (2. - 0.25 * 1.1875)).abs() < 1e-6);
        let mut invalid = head;
        invalid.rotation = [0.; 4];
        assert!(skull_bone(invalid).is_none());
    }
    #[test]
    fn native_adult_offsets_mirror_and_slim_shift_precedes_arm_rotation() {
        let right = held_bone(identity(), identity(), false, false).unwrap();
        let left = held_bone(identity(), identity(), true, false).unwrap();
        for (b, x) in [(right, -1. / 16.), (left, 1. / 16.)] {
            let p = b.translation_scale;
            assert!((p[0] - x).abs() < 1e-6);
            assert!((p[1] - 10. / 16.).abs() < 1e-6);
            assert!((p[2] + 2. / 16.).abs() < 1e-6);
        }
        let mut turned = identity();
        turned.rotation = Quat::from_rotation_z(1.).to_array();
        let a = held_bone(turned, identity(), false, false).unwrap();
        let b = held_bone(turned, identity(), false, true).unwrap();
        assert!((b.translation_scale[0] - a.translation_scale[0] - 0.5 / 16.).abs() < 1e-6);
        assert!((b.translation_scale[1] - a.translation_scale[1]).abs() < 1e-6);
    }
}
