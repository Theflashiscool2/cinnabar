//! Native Java statue parts, retaining the verified model hierarchy and UV/winding data.
use super::{atlas::BlockEntityAtlas, mesh::MeshBuilder};
use assets::{entity::java::Part, registry::java::statue::Statue};
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Model {
    pub state: Statue,
    pub root: Arc<Part>,
}
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state && Arc::ptr_eq(&self.root, &other.root)
    }
}
pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
) {
    let Some(texture) = builder.required_texture(atlas, model.state.texture, [64.; 2]) else {
        return;
    };
    let transform =
        Mat4::from_translation(Vec3::from_array(block.map(|n| n as f32)) + Vec3::new(0.5, 0., 0.5))
            * Mat4::from_rotation_y(
                -f32::from((model.state.turn + 2) % 4) * std::f32::consts::FRAC_PI_2,
            )
            * Mat4::from_scale(Vec3::splat(1. / 16.));
    let mut offset = model.root.offset;
    offset[1] = 0.;
    let [x, y, _] = model.root.rotation;
    let pose = Mat4::from_scale_rotation_translation(
        Vec3::from_array(model.root.scale),
        Quat::from_euler(EulerRot::ZYX, std::f32::consts::PI, y, x),
        Vec3::from_array(offset),
    );
    super::java_parts::part(builder, &model.root, transform, texture, Some(pose));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn all_native_statue_poses_match_reference_vertices_uvs_and_normals() {
        let (Ok(root), Ok(reference)) = (
            std::env::var("CINNABAR_JAVA_TEST_ASSETS"),
            std::env::var("CINNABAR_JAVA_STATUE_VERTICES"),
        ) else {
            eprintln!(
                "missing fixtures: CINNABAR_JAVA_TEST_ASSETS and CINNABAR_JAVA_STATUE_VERTICES"
            );
            return;
        };
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../java-protocol/data/target.json")).unwrap();
        let pin = &pin["entity_models_report"];
        let models = assets::entity::java::Models::load(
            &std::fs::read(std::path::Path::new(&root).join("models.json")).unwrap(),
            pin["sha256"].as_str().unwrap(),
            pin["bytes"].as_u64().unwrap(),
        )
        .unwrap();
        let images = BTreeMap::from([(
            "statue".into(),
            Arc::new(assets::registry::java::textures::Image {
                width: 64,
                height: 64,
                rgba: vec![255; 64 * 64 * 4],
            }),
        )]);
        let assets = assets::registry::java::special::atlas(&images)
            .unwrap()
            .unwrap();
        let atlas = BlockEntityAtlas::from_assets(&assets);
        let size = atlas.size();
        let fixture = std::fs::read_to_string(reference).unwrap();
        let mut cases = 0;
        for case in fixture.split("END\n").filter(|s| !s.trim().is_empty()) {
            let mut lines = case.lines();
            let header: Vec<_> = lines.next().unwrap().split_whitespace().collect();
            let name = match header[0] {
                "standing" => "statue_standing",
                "sitting" => "statue_sitting",
                "running" => "statue_running",
                "star" => "statue_star",
                _ => panic!("unexpected reference pose"),
            };
            let model = Model {
                state: Statue {
                    texture: "statue",
                    model: name,
                    turn: header[1].parse().unwrap(),
                },
                root: Arc::new(models.get(name).unwrap().clone()),
            };
            let mut mesh = MeshBuilder::new(size);
            mesh.actor_light = 1;
            emit(&mut mesh, &atlas, [0; 3], &model);
            let expected: Vec<Vec<f32>> = lines
                .map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect())
                .collect();
            assert_eq!(mesh.solid.len(), expected.len() * 3 / 2);
            let actual: Vec<_> = mesh
                .solid
                .iter()
                .map(|v| {
                    [
                        v.position[0],
                        v.position[1],
                        v.position[2],
                        v.uv[0] * size[0] as f32 / 64.,
                        v.uv[1] * size[1] as f32 / 64.,
                        v.normal[0],
                        v.normal[1],
                        v.normal[2],
                    ]
                })
                .collect();
            let close = |a: &[f32], b: &[f32]| {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.00001)
            };
            for v in &expected {
                assert!(
                    actual.iter().any(|a| close(v, a)),
                    "missing native {header:?}: {v:?}"
                );
            }
            for v in &actual {
                assert!(
                    expected.iter().any(|a| close(v, a)),
                    "unexpected {header:?}: {v:?}"
                );
            }
            let mut translated = MeshBuilder::new(size);
            translated.actor_light = 1;
            emit(&mut translated, &atlas, [-16, 32, -1], &model);
            for (a, b) in mesh.solid.iter().zip(&translated.solid) {
                assert!((b.position[0] - a.position[0] + 16.).abs() < 1e-5);
                assert!((b.position[1] - a.position[1] - 32.).abs() < 1e-5);
                assert!((b.position[2] - a.position[2] + 1.).abs() < 1e-5);
                assert_eq!(a.uv, b.uv);
            }
            cases += 1;
        }
        assert_eq!(cases, 16);
    }
}
