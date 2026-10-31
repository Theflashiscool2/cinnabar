//! Java 26.3 box shell, six attachment transforms and authoritative opening progress.
//! Geometry/UV/poses checked against ShulkerModel.createBoxLayer and ShulkerBoxRenderer.
use super::{atlas::BlockEntityAtlas, java_cube::cuboid, mesh::MeshBuilder};
use assets::{block_entity_geometry::BoxSpec, registry::java::special::Shulker};
use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub state: Shulker,
    pub open: f32,
}

/// Direction.getRotation includes roll about the attachment axis, not just a normal.
fn facing(direction: u8) -> Quat {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    match direction {
        0 => Quat::from_xyzw(1., 0., 0., 0.),
        1 => Quat::IDENTITY,
        2 => Quat::from_xyzw(0., -h, h, 0.),
        3 => Quat::from_xyzw(h, 0., 0., h),
        4 => Quat::from_xyzw(0.5, -0.5, 0.5, 0.5),
        5 => Quat::from_xyzw(0.5, 0.5, -0.5, 0.5),
        _ => unreachable!("native shulker facing validated during admission"),
    }
}

pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
) {
    let Some(texture) = builder.required_texture(atlas, &model.state.texture, [64.; 2]) else {
        return;
    };
    let base = Mat4::from_translation(Vec3::from_array(block.map(|n| n as f32)) + Vec3::splat(0.5))
        * Mat4::from_scale(Vec3::splat(0.9995))
        * Mat4::from_quat(facing(model.state.facing))
        * Mat4::from_scale(Vec3::new(1., -1., -1.))
        * Mat4::from_translation(Vec3::new(0., -1., 0.))
        * Mat4::from_scale(Vec3::splat(1. / 16.));
    cuboid(
        builder,
        &texture,
        base * Mat4::from_translation(Vec3::new(0., 24., 0.)),
        BoxSpec::new([-8.; 3], [16., 8., 16.], [0., 28.]),
        63,
    );
    let open = model.open.clamp(0., 1.);
    let lid = base
        * Mat4::from_translation(Vec3::new(0., 24. - 8. * open, 0.))
        * Mat4::from_rotation_y((270. * open).to_radians());
    cuboid(
        builder,
        &texture,
        lid,
        BoxSpec::new([-8., -16., -8.], [16., 12., 16.], [0., 0.]),
        63,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_entity::{
        BlockEntityKind, BlockEntityLight, BlockEntityScene, BlockEntitySubmission, SceneClock,
    };
    use std::{collections::BTreeMap, sync::Arc};
    fn scene() -> BlockEntityScene {
        let images = BTreeMap::from([(
            "textures/entity/shulker/shulker".into(),
            Arc::new(assets::registry::java::textures::Image {
                width: 64,
                height: 64,
                rgba: vec![255; 64 * 64 * 4],
            }),
        )]);
        let mut s = BlockEntityScene::default();
        s.install_assets(
            &assets::registry::java::special::atlas(&images)
                .unwrap()
                .unwrap(),
        );
        s
    }
    fn sub(at: [i32; 3], direction: u8, open: f32) -> BlockEntitySubmission {
        BlockEntitySubmission {
            block: at,
            light: BlockEntityLight::Actor { block: 0, sky: 15 },
            kind: BlockEntityKind::JavaShulker(Model {
                state: Shulker {
                    block_id: 0,
                    texture: "textures/entity/shulker/shulker".into(),
                    facing: direction,
                },
                open,
            }),
        }
    }
    #[test]
    fn closed_shell_and_open_lid_follow_all_six_reference_axes() {
        let mut s = scene();
        for (dir, axis, sign) in [
            (0, 1, -1.),
            (1, 1, 1.),
            (2, 2, -1.),
            (3, 2, 1.),
            (4, 0, -1.),
            (5, 0, 1.),
        ] {
            for open in [0., 0.5, 1.] {
                let f = s.update(SceneClock::default(), &[], &[sub([-16, 32, -1], dir, open)]);
                assert_eq!(f.solid.len(), 72);
                let bounds = f.solid.iter().fold(
                    ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
                    |(mut lo, mut hi), v| {
                        for i in 0..3 {
                            let p = v.position[i] - [-16., 32., -1.][i];
                            lo[i] = lo[i].min(p);
                            hi[i] = hi[i].max(p);
                        }
                        assert!(v.position.iter().chain(&v.uv).all(|n| n.is_finite()));
                        (lo, hi)
                    },
                );
                if open == 0. {
                    for i in 0..3 {
                        assert!((bounds.0[i] - 0.00025).abs() < 1e-5);
                        assert!((bounds.1[i] - 0.99975).abs() < 1e-5);
                    }
                }
                let extreme = if sign > 0. {
                    bounds.1[axis]
                } else {
                    1. - bounds.0[axis]
                };
                assert!(
                    (extreme - (0.99975 + 0.49975 * open)).abs() < 1e-4,
                    "direction={dir}, progress={open}"
                );
            }
        }
    }
    #[test]
    fn moved_shell_keeps_uvs_and_does_not_retain_removed_geometry() {
        let mut s = scene();
        let a = s
            .update(SceneClock::default(), &[], &[sub([0; 3], 4, 0.5)])
            .clone();
        let b = s.update(SceneClock::default(), &[], &[sub([-17, 16, 31], 4, 0.5)]);
        for (a, b) in a.solid.iter().zip(b.solid.iter()) {
            assert_eq!(a.uv, b.uv);
            for i in 0..3 {
                assert!((b.position[i] - a.position[i] - [-17., 16., 31.][i]).abs() < 1e-4);
            }
        }
        assert!(s.update(SceneClock::default(), &[], &[]).solid.is_empty());
    }
}
