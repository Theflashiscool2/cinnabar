//! Native Java conduit transforms; shared atlas/mesh/GPU ownership only.
use super::{atlas::BlockEntityAtlas, mesh::MeshBuilder};
use assets::registry::java::conduit::{Resources, TEXTURES};
use glam::{Mat4, Quat, Vec3};
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct Model {
    pub resources: Arc<Resources>,
    pub active: bool,
    pub hunting: bool,
    pub ticks: f32,
    pub texture_ticks: f64,
    pub rotation: f32,
    pub camera: Quat,
}
impl PartialEq for Model {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.resources, &o.resources)
            && self.active == o.active
            && self.hunting == o.hunting
            && self.texture_ticks == o.texture_ticks
            && self.ticks == o.ticks
            && self.rotation == o.rotation
            && self.camera == o.camera
    }
}
/// Returns native model index, texture, logical dimensions and pixel-space transform.
fn instances(model: &Model) -> Vec<(usize, &str, [f32; 2], Mat4)> {
    let center = Mat4::from_translation(Vec3::splat(0.5));
    let pixels = Mat4::from_scale(Vec3::splat(1. / 16.));
    if !model.active {
        return vec![(
            0,
            TEXTURES[0],
            [32., 16.],
            center * Mat4::from_rotation_y(model.rotation.to_radians()) * pixels,
        )];
    }
    let bob = (model.ticks * 0.1).sin() / 2. + 0.5;
    let floating = Mat4::from_translation(Vec3::new(0.5, 0.3 + (bob * bob + bob) * 0.2, 0.5));
    let phase = ((model.ticks.floor() as u64) / 66) % 3;
    let wind = &model.resources.winds[usize::from(phase == 1)];
    let index = wind.timeline[((model.texture_ticks.max(0.) as u64 / u64::from(wind.ticks))
        % wind.timeline.len() as u64) as usize];
    let texture = wind.frames[index].as_str();
    let turn = match phase {
        1 => Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
        2 => Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
        _ => Mat4::IDENTITY,
    };
    vec![
        (
            1,
            TEXTURES[1],
            [32., 16.],
            floating
                * Mat4::from_quat(Quat::from_axis_angle(
                    Vec3::new(0.5, 1., 0.5).normalize(),
                    model.rotation,
                ))
                * pixels,
        ),
        (2, texture, [64., 32.], center * turn * pixels),
        (
            2,
            texture,
            [64., 32.],
            center
                * Mat4::from_scale(Vec3::splat(0.875))
                * Mat4::from_rotation_x(std::f32::consts::PI)
                * Mat4::from_rotation_z(std::f32::consts::PI)
                * pixels,
        ),
        (
            3,
            TEXTURES[if model.hunting { 4 } else { 5 }],
            [16.; 2],
            floating
                * Mat4::from_scale(Vec3::splat(0.5))
                * Mat4::from_quat(model.camera)
                * Mat4::from_rotation_z(std::f32::consts::PI)
                * Mat4::from_rotation_y(std::f32::consts::PI)
                * Mat4::from_scale(Vec3::splat(4. / 3.))
                * pixels,
        ),
    ]
}
pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
) {
    let world = Mat4::from_translation(Vec3::from_array(block.map(|n| n as f32)));
    for (part, name, logical, transform) in instances(model) {
        if let Some(texture) = builder.required_texture(atlas, name, logical) {
            super::java_parts::part(
                builder,
                &model.resources.parts[part],
                world * transform,
                texture,
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn native_layers_transforms_uvs_normals_and_translation() {
        let (Ok(root), Ok(reference)) = (
            std::env::var("CINNABAR_JAVA_TEST_ASSETS"),
            std::env::var("CINNABAR_JAVA_CONDUIT_VERTICES"),
        ) else {
            eprintln!("missing fixtures: CINNABAR_JAVA_TEST_ASSETS/CINNABAR_JAVA_CONDUIT_VERTICES");
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
        let resources = Arc::new(Resources {
            parts: assets::registry::java::conduit::MODELS
                .map(|n| Arc::new(models.get(n).unwrap().clone())),
            winds: std::array::from_fn(|_| assets::registry::java::conduit::Wind {
                frames: vec!["test".into()],
                timeline: vec![0],
                ticks: 3,
            }),
        });
        let images = BTreeMap::from([(
            "test".into(),
            Arc::new(assets::registry::java::textures::Image {
                width: 32,
                height: 32,
                rgba: vec![255; 4096],
            }),
        )]);
        let artwork = assets::registry::java::special::atlas(&images)
            .unwrap()
            .unwrap();
        let atlas = BlockEntityAtlas::from_assets(&artwork);
        let size = atlas.size();
        let text = std::fs::read_to_string(reference).unwrap();
        let mut count = 0;
        for case in text.split("END\n").filter(|s| !s.trim().is_empty()) {
            let mut lines = case.lines();
            let header: Vec<usize> = lines
                .next()
                .unwrap()
                .split_whitespace()
                .map(|s| s.parse().unwrap())
                .collect();
            let ticks = [0., 15., 80., 150.][header[0]];
            let model = Model {
                resources: Arc::clone(&resources),
                active: header[0] > 0,
                hunting: header[0] == 3,
                ticks,
                texture_ticks: f64::from(ticks),
                rotation: ticks * -0.0375,
                camera: Quat::from_rotation_y(0.7) * Quat::from_rotation_x(-0.2),
            };
            let (part, _, _, transform) = instances(&model)[header[1]];
            let expected: Vec<Vec<f32>> = lines
                .map(|l| l.split_whitespace().map(|n| n.parse().unwrap()).collect())
                .collect();
            for offset in [Vec3::ZERO, Vec3::new(-17., -64., 31.)] {
                let mut mesh = MeshBuilder::new(size);
                mesh.actor_light = 1;
                let texture = mesh.required_texture(&atlas, "test", [32.; 2]).unwrap();
                super::super::java_parts::part(
                    &mut mesh,
                    &resources.parts[part],
                    Mat4::from_translation(offset) * transform,
                    texture,
                    None,
                );
                let actual: Vec<_> = mesh
                    .solid
                    .iter()
                    .map(|v| {
                        [
                            v.position[0] - offset.x,
                            v.position[1] - offset.y,
                            v.position[2] - offset.z,
                            v.uv[0] * size[0] as f32 / 32.,
                            v.uv[1] * size[1] as f32 / 32.,
                            v.normal[0],
                            v.normal[1],
                            v.normal[2],
                        ]
                    })
                    .collect();
                let close =
                    |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.00003);
                assert_eq!(actual.len(), expected.len() * 3 / 2);
                for v in &expected {
                    assert!(
                        actual.iter().any(|a| close(v, a)),
                        "missing native {header:?} offset={offset:?} {v:?}; nearest {:?}",
                        actual.iter().min_by(|a, b| a
                            .iter()
                            .zip(v)
                            .map(|(x, y)| (x - y).abs())
                            .sum::<f32>()
                            .total_cmp(&b.iter().zip(v).map(|(x, y)| (x - y).abs()).sum::<f32>()))
                    );
                }
                for v in &actual {
                    assert!(
                        expected.iter().any(|a| close(v, a)),
                        "unexpected native {header:?} {v:?}"
                    );
                }
            }
            count += 1;
        }
        assert_eq!(count, 13);
    }
}
