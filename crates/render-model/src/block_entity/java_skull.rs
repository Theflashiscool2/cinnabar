//! Java SkullModel mounts and UVs; separate from Bedrock head geometry and skin ownership.
use super::{
    atlas::BlockEntityAtlas,
    java_cube::faces,
    mesh::{Layer, MeshBuilder, WHITE},
};
use assets::{
    block_entity_geometry::BoxSpec,
    registry::java::skull::{Shape, Skull},
};
use glam::{Mat4, Vec3};

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub state: Skull,
    pub animation: f32,
    pub texture: Option<std::sync::Arc<str>>,
}

pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
) {
    let state = &model.state;
    let texture_size = match state.shape {
        Shape::Cube => [64., 32.],
        Shape::Human | Shape::Player | Shape::Piglin => [64., 64.],
        Shape::Dragon => [256.; 2],
    };
    let Some(texture) = builder.required_texture(
        atlas,
        model.texture.as_deref().unwrap_or(state.texture),
        texture_size,
    ) else {
        return;
    };
    let (offset, rotation) = if state.wall {
        let normal = match state.rotation {
            0 => Vec3::Z,
            4 => Vec3::NEG_X,
            8 => Vec3::NEG_Z,
            12 => Vec3::X,
            _ => unreachable!("admitted native wall skull"),
        };
        (
            Vec3::new(0.5, 0.25, 0.5) - normal * 0.25,
            (state.rotation + 8) % 16,
        )
    } else {
        (Vec3::new(0.5, 0., 0.5), state.rotation)
    };
    let base = Mat4::from_translation(Vec3::from_array(block.map(|x| x as f32)) + offset)
        * Mat4::from_rotation_y(-(rotation as f32 * 22.5).to_radians())
        * Mat4::from_scale(Vec3::new(-1., -1., 1.) / 16.);
    let mut draw = |pose: Mat4, spec: BoxSpec, inflate: f32, mirror: bool| {
        for ((corners, _), (_, texels)) in
            faces(spec.inflated(inflate)).into_iter().zip(faces(spec))
        {
            let mut corners = corners;
            let [u0, v0, u1, v1] = texture.rect_uv(texels);
            let mut uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
            if mirror {
                for point in &mut corners {
                    point[0] = 2. * spec.origin[0] + spec.size[0] - point[0];
                }
                corners.reverse();
                uv.reverse();
            }
            builder.quad_uv(
                if state.shape == Shape::Player {
                    Layer::Translucent
                } else {
                    Layer::Solid
                },
                corners.map(|p| pose.transform_point3(Vec3::from_array(p)).to_array()),
                uv,
                WHITE,
            );
        }
    };
    match state.shape {
        Shape::Cube | Shape::Human | Shape::Player => {
            draw(
                base,
                BoxSpec::new([-4., -8., -4.], [8.; 3], [0., 0.]),
                0.,
                false,
            );
            if matches!(state.shape, Shape::Human | Shape::Player) {
                draw(
                    base,
                    BoxSpec::new([-4., -8., -4.], [8.; 3], [32., 0.]),
                    0.25,
                    false,
                );
            }
        }
        Shape::Piglin => {
            for (at, size, uv) in [
                ([-5., -8., -4.], [10., 8., 8.], [0., 0.]),
                ([-2., -4., -5.], [4., 4., 1.], [31., 1.]),
                ([2., -2., -5.], [1., 2., 1.], [2., 4.]),
                ([-3., -2., -5.], [1., 2., 1.], [2., 0.]),
            ] {
                draw(base, BoxSpec::new(at, size, uv), 0., false);
            }
            let phase = model.animation * std::f32::consts::PI * 0.2;
            let left = -((phase * 1.2).cos() + 2.5) * 0.2;
            let right = (phase.cos() + 2.5) * 0.2;
            draw(
                base * Mat4::from_translation(Vec3::new(4.5, -6., 0.))
                    * Mat4::from_rotation_z(left),
                BoxSpec::new([0., 0., -2.], [1., 5., 4.], [51., 6.]),
                0.,
                false,
            );
            draw(
                base * Mat4::from_translation(Vec3::new(-4.5, -6., 0.))
                    * Mat4::from_rotation_z(right),
                BoxSpec::new([-1., 0., -2.], [1., 5., 4.], [39., 6.]),
                0.,
                false,
            );
        }
        Shape::Dragon => {
            // PartPose.scaled scales its offset as well: the baked head Y is -5.99.
            let head = base
                * Mat4::from_translation(Vec3::new(0., -5.99, 0.))
                * Mat4::from_scale(Vec3::splat(0.75));
            for (at, size, uv, mirror) in [
                ([-6., -1., -24.], [12., 5., 16.], [176., 44.], false),
                ([-8., -8., -10.], [16.; 3], [112., 30.], false),
                ([-5., -12., -4.], [2., 4., 6.], [0., 0.], true),
                ([-5., -3., -22.], [2., 2., 4.], [112., 0.], true),
                ([3., -12., -4.], [2., 4., 6.], [0., 0.], false),
                ([3., -3., -22.], [2., 2., 4.], [112., 0.], false),
            ] {
                draw(head, BoxSpec::new(at, size, uv), 0., mirror);
            }
            let angle = ((model.animation * std::f32::consts::PI * 0.2).sin() + 1.) * 0.2;
            let jaw = head
                * Mat4::from_translation(Vec3::new(0., 4., -8.))
                * Mat4::from_rotation_x(angle);
            draw(
                jaw,
                BoxSpec::new([-6., 0., -16.], [12., 4., 16.], [176., 65.]),
                0.,
                false,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_entity::{
        BlockEntityKind, BlockEntityLight, BlockEntityScene, BlockEntitySubmission, SceneClock,
    };
    use std::{collections::BTreeMap, sync::Arc};
    /// Expected vertices come from the verified target client ModelPart renderer, not this emitter.
    #[test]
    fn pinned_native_model_vertices_and_uvs() {
        let Ok(path) = std::env::var("CINNABAR_JAVA_HEAD_VERTICES") else {
            eprintln!(
                "missing fixture: CINNABAR_JAVA_HEAD_VERTICES exported by pinned Java ModelPart"
            );
            return;
        };
        let fixture = std::fs::read_to_string(path).unwrap();
        let mut cases = 0;
        for case in fixture.split("END\n").filter(|s| !s.trim().is_empty()) {
            let mut lines = case.lines();
            let head: Vec<_> = lines.next().unwrap().split_whitespace().collect();
            let (shape, width, height) = match head[0] {
                "cube" => (Shape::Cube, 64, 32),
                "human" => (Shape::Human, 64, 64),
                "piglin" => (Shape::Piglin, 64, 64),
                "dragon" => (Shape::Dragon, 256, 256),
                _ => panic!("unexpected reference header"),
            };
            let images = BTreeMap::from([(
                "head".into(),
                Arc::new(assets::registry::java::textures::Image {
                    width,
                    height,
                    rgba: vec![255; (width * height * 4) as usize],
                }),
            )]);
            let assets = assets::registry::java::special::atlas(&images)
                .unwrap()
                .unwrap();
            let atlas = BlockEntityAtlas::from_assets(&assets);
            let size = atlas.size();
            let mut builder = MeshBuilder::new(size);
            emit(
                &mut builder,
                &atlas,
                [0; 3],
                &Model {
                    state: Skull {
                        texture: "head",
                        shape,
                        powered: false,
                        wall: false,
                        rotation: head[2].parse().unwrap(),
                    },
                    animation: head[1].parse().unwrap(),
                    texture: None,
                },
            );
            let expected: Vec<Vec<f32>> = lines
                .map(|line| {
                    line.split_whitespace()
                        .map(|s| s.parse().unwrap())
                        .collect()
                })
                .collect();
            assert_eq!(builder.solid.len(), expected.len() * 3 / 2);
            let actual: Vec<_> = builder
                .solid
                .iter()
                .map(|v| {
                    [
                        v.position[0],
                        v.position[1],
                        v.position[2],
                        v.uv[0] * size[0] as f32 / width as f32,
                        v.uv[1] * size[1] as f32 / height as f32,
                    ]
                })
                .collect();
            let close =
                |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.00001);
            for v in &expected {
                assert!(
                    actual.iter().any(|a| close(v, a)),
                    "missing reference {head:?}: {v:?}"
                );
            }
            for v in &actual {
                assert!(
                    expected.iter().any(|a| close(v, a)),
                    "unexpected vertex {head:?}: {v:?}"
                );
            }
            if head[0] == "human" {
                let mut player = MeshBuilder::new(size);
                emit(
                    &mut player,
                    &atlas,
                    [0; 3],
                    &Model {
                        state: Skull {
                            texture: "head",
                            shape: Shape::Player,
                            powered: false,
                            wall: false,
                            rotation: head[2].parse().unwrap(),
                        },
                        animation: 0.,
                        texture: None,
                    },
                );
                assert!(player.solid.is_empty());
                assert_eq!(player.translucent, builder.solid);
            }
            cases += 1;
        }
        assert_eq!(cases, 36);
    }
    #[test]
    fn native_skull_mount_bounds_hat_uvs_and_replacement() {
        let images = BTreeMap::from([(
            "textures/entity/zombie/zombie".into(),
            Arc::new(assets::registry::java::textures::Image {
                width: 64,
                height: 64,
                rgba: vec![255; 64 * 64 * 4],
            }),
        )]);
        let mut scene = BlockEntityScene::default();
        scene.install_assets(
            &assets::registry::java::special::atlas(&images)
                .unwrap()
                .unwrap(),
        );
        for rotation in [0, 4, 8, 12] {
            for wall in [false, true] {
                for hat in [false, true] {
                    let sub = BlockEntitySubmission {
                        block: [-16, 32, -1],
                        light: BlockEntityLight::Actor { block: 0, sky: 15 },
                        kind: BlockEntityKind::JavaSkull(Model {
                            state: Skull {
                                texture: "textures/entity/zombie/zombie",
                                shape: if hat { Shape::Human } else { Shape::Cube },
                                powered: false,
                                wall,
                                rotation,
                            },
                            animation: 0.,
                            texture: None,
                        }),
                    };
                    let frame = scene.update(SceneClock::default(), &[], &[sub]);
                    assert_eq!(frame.solid.len(), if hat { 72 } else { 36 });
                    let lo = frame
                        .solid
                        .iter()
                        .map(|v| v.position[1] - 32.)
                        .fold(f32::INFINITY, f32::min);
                    let hi = frame
                        .solid
                        .iter()
                        .map(|v| v.position[1] - 32.)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let center = if wall { 0.25 } else { 0. };
                    let padding = if hat { 0.25 / 16. } else { 0. };
                    assert!((lo - (center - padding)).abs() < 1e-5);
                    assert!((hi - (center + 0.5 + padding)).abs() < 1e-5);
                    assert!(
                        frame.solid.iter().all(|v| v
                            .position
                            .iter()
                            .chain(&v.uv)
                            .all(|x| x.is_finite()))
                    );
                }
            }
        }
        assert!(
            scene
                .update(SceneClock::default(), &[], &[])
                .solid
                .is_empty()
        );
    }
    #[test]
    fn player_head_uses_translucent_hat_and_replaces_texture_without_stale_fragments() {
        let images = BTreeMap::from([(
            "custom".into(),
            Arc::new(assets::registry::java::textures::Image {
                width: 64,
                height: 64,
                rgba: vec![255; 64 * 64 * 4],
            }),
        )]);
        let mut scene = BlockEntityScene::default();
        scene.install_assets(
            &assets::registry::java::special::atlas(&images)
                .unwrap()
                .unwrap(),
        );
        let mut submission = BlockEntitySubmission {
            block: [-16, 32, -1],
            light: BlockEntityLight::Actor { block: 7, sky: 15 },
            kind: BlockEntityKind::JavaSkull(Model {
                state: Skull {
                    texture: "unused",
                    shape: Shape::Player,
                    powered: false,
                    wall: false,
                    rotation: 0,
                },
                animation: 0.,
                texture: Some("custom".into()),
            }),
        };
        for rotation in 0..16 {
            let BlockEntityKind::JavaSkull(model) = &mut submission.kind else {
                unreachable!()
            };
            model.state.rotation = rotation;
            let frame = scene.update(SceneClock::default(), &[], &[submission.clone()]);
            assert!(frame.solid.is_empty());
            assert_eq!(frame.translucent.len(), 72);
            assert!(frame.translucent.iter().all(
                |v| v.actor_light != 0 && v.position.iter().chain(&v.uv).all(|x| x.is_finite())
            ));
        }
        for rotation in [0, 4, 8, 12] {
            let BlockEntityKind::JavaSkull(model) = &mut submission.kind else {
                unreachable!()
            };
            model.state.wall = true;
            model.state.rotation = rotation;
            let frame = scene.update(SceneClock::default(), &[], &[submission.clone()]);
            assert_eq!(frame.translucent.len(), 72);
            let min = frame
                .translucent
                .iter()
                .map(|v| v.position[1])
                .fold(f32::INFINITY, f32::min);
            assert!((min - 32.234375).abs() < 1e-5);
        }
        assert!(
            scene
                .update(SceneClock::default(), &[], &[])
                .translucent
                .is_empty()
        );
    }
}
