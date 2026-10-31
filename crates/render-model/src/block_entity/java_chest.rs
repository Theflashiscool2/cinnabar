//! Java 26.3 ChestModel coordinates and separate 64x64 half textures.
//! Original geometry emitter; facts checked against the verified client's layer definitions.
use super::{atlas::BlockEntityAtlas, java_cube::cuboid, mesh::MeshBuilder};
use assets::{
    block_entity_geometry::BoxSpec,
    registry::java::special::{Chest, ChestHalf},
};
use glam::{Mat4, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub state: Chest,
    /// Resolved against the resident authoritative partner; absent/mismatched partners use single.
    pub half: ChestHalf,
    pub lid: f32,
}

pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
) {
    let Some(texture) =
        builder.required_texture(atlas, &model.state.texture_path(model.half), [64.; 2])
    else {
        return;
    };
    let base =
        Mat4::from_translation(Vec3::from_array(block.map(|v| v as f32)) + Vec3::new(0.5, 0., 0.5))
            * Mat4::from_rotation_y(-(model.state.turn as f32) * std::f32::consts::FRAC_PI_2)
            * Mat4::from_translation(Vec3::new(-0.5, 0., -0.5))
            * Mat4::from_scale(Vec3::splat(1. / 16.));
    let (x, width, latch_x, latch_width, mask) = match model.half {
        ChestHalf::Single => (1., 14., 7., 2., 63),
        ChestHalf::Left => (0., 15., 0., 1., 63 & !(1 << 3)),
        ChestHalf::Right => (1., 15., 15., 1., 63 & !(1 << 2)),
    };
    cuboid(
        builder,
        &texture,
        base,
        BoxSpec::new([x, 0., 1.], [width, 10., 14.], [0., 19.]),
        mask,
    );
    let lid = base
        * Mat4::from_translation(Vec3::new(0., 9., 1.))
        * Mat4::from_rotation_x(-super::chest::lid_angle_radians(model.lid));
    cuboid(
        builder,
        &texture,
        lid,
        BoxSpec::new([x, 0., 0.], [width, 5., 14.], [0., 0.]),
        mask,
    );
    cuboid(
        builder,
        &texture,
        lid,
        BoxSpec::new([latch_x, -2., 14.], [latch_width, 4., 1.], [0., 0.]),
        mask,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_entity::{
        BlockEntityKind, BlockEntityLight, BlockEntityScene, BlockEntitySubmission, SceneClock,
    };
    use std::{collections::BTreeMap, sync::Arc};
    #[test]
    fn java_cube_front_texels_follow_the_reference_not_bedrock_unwrap() {
        let fs =
            super::super::java_cube::faces(BoxSpec::new([1., 0., 1.], [14., 10., 14.], [0., 19.]));
        assert_eq!(
            fs[1].0,
            [
                [1., 10., 15.],
                [15., 10., 15.],
                [15., 0., 15.],
                [1., 0., 15.]
            ]
        );
        assert_eq!(fs[1].1, [56., 43., -14., -10.]);
        assert_eq!(fs[2].1, [42., 43., -14., -10.]);
        assert_eq!(fs[4].1, [42., 19., -14., 14.]);
    }
    fn scene() -> BlockEntityScene {
        let mut images = BTreeMap::new();
        for suffix in ["", "_left", "_right"] {
            images.insert(
                format!("textures/entity/chest/normal{suffix}"),
                Arc::new(assets::registry::java::textures::Image {
                    width: 64,
                    height: 64,
                    rgba: vec![255; 64 * 64 * 4],
                }),
            );
        }
        let atlas = assets::registry::java::special::atlas(&images)
            .unwrap()
            .unwrap();
        let mut scene = BlockEntityScene::default();
        scene.install_assets(&atlas);
        scene
    }
    fn state(turn: u8, half: ChestHalf, lid: f32) -> Model {
        Model {
            state: Chest {
                block_id: 0,
                block: "minecraft:chest".into(),
                texture: "normal".into(),
                turn,
                half,
            },
            half,
            lid,
        }
    }
    fn submission(block: [i32; 3], model: Model) -> BlockEntitySubmission {
        BlockEntitySubmission {
            block,
            light: BlockEntityLight::Actor { block: 0, sky: 15 },
            kind: BlockEntityKind::JavaChest(model),
        }
    }
    #[test]
    fn java_chest_closed_dimensions_and_all_four_rotations_are_geometric() {
        let mut scene = scene();
        for turn in 0..4 {
            let frame = scene.update(
                SceneClock { ticks: 0. },
                &[],
                &[submission(
                    [-16, 32, -1],
                    state(turn, ChestHalf::Single, 0.),
                )],
            );
            assert_eq!(frame.solid.len(), 108);
            let bounds = frame.solid.iter().fold(
                ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
                |(mut lo, mut hi), v| {
                    for i in 0..3 {
                        lo[i] = lo[i].min(v.position[i]);
                        hi[i] = hi[i].max(v.position[i]);
                    }
                    (lo, hi)
                },
            );
            assert!((bounds.0[1] - 32.).abs() < 1e-5);
            assert!((bounds.1[1] - 32.875).abs() < 1e-5);
            let front = match turn {
                0 => (2, 0., true),
                1 => (0, -16., false),
                2 => (2, -1., false),
                _ => (0, -15., true),
            };
            assert!(
                (if front.2 {
                    bounds.1[front.0]
                } else {
                    bounds.0[front.0]
                } - front.1)
                    .abs()
                    < 1e-5
            );
            assert!(
                frame
                    .solid
                    .iter()
                    .all(|v| v.position.iter().chain(&v.uv).all(|v| v.is_finite())
                        && v.uv.iter().all(|v| (0.0..=1.0).contains(v)))
            );
        }
    }
    #[test]
    fn double_halves_omit_the_shared_wall_and_lid_uses_the_back_hinge() {
        let mut scene = scene();
        let pair = [
            submission([0; 3], state(0, ChestHalf::Right, 0.)),
            submission([1, 0, 0], state(0, ChestHalf::Left, 0.)),
        ];
        let frame = scene.update(SceneClock { ticks: 0. }, &[], &pair);
        assert_eq!(frame.solid.len(), 180); // two x three cuboids x five exterior faces x six vertices.
        for triangle in frame.solid.chunks_exact(3) {
            assert!(
                !triangle.iter().all(|v| (v.position[0] - 1.).abs() < 1e-6),
                "internal coplanar seam face"
            );
        }
        let opened = scene.update(
            SceneClock { ticks: 1. },
            &[],
            &[submission([0; 3], state(0, ChestHalf::Single, 1.))],
        );
        let max_y = opened
            .solid
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_y - 1.5).abs() < 1e-5,
            "latch rotates upward about y9,z1 hinge"
        );
        let empty = scene.update(SceneClock { ticks: 2. }, &[], &[]);
        assert!(empty.solid.is_empty());
    }
}
