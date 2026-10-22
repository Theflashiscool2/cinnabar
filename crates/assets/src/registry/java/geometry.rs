//! Bakes Java cuboid geometry before conversion to Cinnabar's shared GPU model format.
use super::{
    models::{Element, Face, Material, Model},
    selectors::Variant,
    source::Error,
};
use glam::{Mat3, Vec3};
#[derive(Clone, Debug)]
pub struct Quad {
    pub positions: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub material: Material,
    pub facing: Face,
    pub cull_face: Option<Face>,
    pub tint_index: i32,
    pub shade_override: Option<Face>,
    pub light_emission: u8,
    pub ambient_occlusion: bool,
}
pub fn bake(model: &Model, variant: &Variant) -> Result<Vec<Quad>, Error> {
    let [x, y, z] = variant
        .rotations
        .map(|degrees| -(degrees as f32).to_radians());
    let rotation = Mat3::from_rotation_z(z) * Mat3::from_rotation_y(y) * Mat3::from_rotation_x(x);
    let mut quads = Vec::new();
    for element in model.elements.iter() {
        let local = element_transform(element)?;
        for (face, definition) in &element.faces {
            let positions = vertices(*face, element.from, element.to).map(|p| {
                let mut p = Vec3::from_array(p) / 16.;
                if let Some((origin, transform)) = &local {
                    p = *transform * (p - *origin) + *origin;
                }
                (rotation * (p - Vec3::splat(0.5)) + Vec3::splat(0.5)).to_array()
            });
            let facing = nearest(rotation * normal(*face));
            let [u0, v0, u1, v1] = definition
                .uv
                .unwrap_or_else(|| default_uv(*face, element.from, element.to));
            let raw = [[u0, v0], [u0, v1], [u1, v1], [u1, v0]];
            let shift = definition.rotation.rem_euclid(360) as usize / 90;
            let uv_transform = if variant.uv_lock {
                (basis(facing).transpose() * rotation * basis(*face)).transpose()
            } else {
                Mat3::IDENTITY
            };
            let uvs = std::array::from_fn(|i| {
                let [u, v] = raw[(i + shift) % 4];
                let p = uv_transform * Vec3::new(u / 16. - 0.5, v / 16. - 0.5, 0.);
                [p.x + 0.5, p.y + 0.5]
            });
            if positions
                .iter()
                .flatten()
                .chain(uvs.iter().flatten())
                .any(|v| !v.is_finite())
            {
                return Err(Error::Model("nonfinite baked geometry/UV".into()));
            }
            let geometric_normal = (Vec3::from_array(positions[1])
                - Vec3::from_array(positions[0]))
            .cross(Vec3::from_array(positions[2]) - Vec3::from_array(positions[0]));
            quads.push(Quad {
                positions,
                uvs,
                material: model.material(&definition.texture)?,
                facing: nearest(geometric_normal),
                cull_face: definition
                    .cullface
                    .map(|face| nearest(rotation * normal(face))),
                tint_index: definition.tintindex,
                shade_override: element.shade_direction_override,
                light_emission: element.light_emission,
                ambient_occlusion: model.ambient_occlusion,
            });
        }
    }
    Ok(quads)
}
fn element_transform(element: &Element) -> Result<Option<(Vec3, Mat3)>, Error> {
    let Some(rotation) = &element.rotation else {
        return Ok(None);
    };
    let [x, y, z] = rotation.angles()?.map(f32::to_radians);
    let mut matrix = Mat3::from_rotation_z(z) * Mat3::from_rotation_y(y) * Mat3::from_rotation_x(x);
    if rotation.rescale {
        let scale = Vec3::new(
            matrix.x_axis.abs().max_element().recip(),
            matrix.y_axis.abs().max_element().recip(),
            matrix.z_axis.abs().max_element().recip(),
        );
        matrix *= Mat3::from_diagonal(scale);
    }
    Ok(Some((Vec3::from_array(rotation.origin) / 16., matrix)))
}
pub fn normal(face: Face) -> Vec3 {
    match face {
        Face::Down => Vec3::NEG_Y,
        Face::Up => Vec3::Y,
        Face::West => Vec3::NEG_X,
        Face::East => Vec3::X,
        Face::North => Vec3::NEG_Z,
        Face::South => Vec3::Z,
    }
}
fn nearest(n: Vec3) -> Face {
    let mut best = Face::Up;
    let mut dot = 0.;
    for face in [
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
        Face::West,
        Face::East,
    ] {
        let d = n.dot(normal(face));
        if d > dot {
            dot = d;
            best = face;
        }
    }
    best
}
/// Face-local U/V/Z frames used by Java's BlockMath UV-lock transform.
fn basis(face: Face) -> Mat3 {
    match face {
        Face::South => Mat3::IDENTITY,
        Face::East => Mat3::from_cols(Vec3::NEG_Z, Vec3::Y, Vec3::X),
        Face::West => Mat3::from_cols(Vec3::Z, Vec3::Y, Vec3::NEG_X),
        Face::North => Mat3::from_cols(Vec3::NEG_X, Vec3::Y, Vec3::NEG_Z),
        Face::Up => Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::Y),
        Face::Down => Mat3::from_cols(Vec3::X, Vec3::Z, Vec3::NEG_Y),
    }
}
fn vertices(face: Face, [x0, y0, z0]: [f32; 3], [x1, y1, z1]: [f32; 3]) -> [[f32; 3]; 4] {
    match face {
        Face::Down => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        Face::Up => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        Face::West => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        Face::East => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
        Face::North => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        Face::South => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
    }
}
fn default_uv(face: Face, [x0, y0, z0]: [f32; 3], [x1, y1, z1]: [f32; 3]) -> [f32; 4] {
    match face {
        Face::Down => [x0, 16. - z1, x1, 16. - z0],
        Face::Up => [x0, z0, x1, z1],
        Face::North => [16. - x1, 16. - y1, 16. - x0, 16. - y0],
        Face::South => [x0, 16. - y1, x1, 16. - y0],
        Face::West => [z0, 16. - y1, z1, 16. - y0],
        Face::East => [16. - z1, 16. - y1, 16. - z0, 16. - y0],
    }
}

#[cfg(test)]
mod tests {
    use super::super::source::Identifier;
    use super::*;
    use std::{collections::BTreeMap, sync::Arc};
    fn model() -> Model {
        let element:Element=serde_json::from_str(r##"{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#top","cullface":"up","tintindex":0}}}"##).unwrap();
        Model {
            elements: Arc::from([element]),
            textures: BTreeMap::from([("top".into(), serde_json::json!("test:top"))]),
            ambient_occlusion: true,
        }
    }
    fn variant() -> Variant {
        Variant {
            model: Identifier::parse("test:fixture").unwrap(),
            rotations: [0, 0, 0],
            uv_lock: false,
            weight: 1,
        }
    }
    fn close(actual: [f32; 2], expected: [f32; 2]) {
        for (a, b) in actual.into_iter().zip(expected) {
            assert!((a - b).abs() < 0.00001, "{a} != {b}");
        }
    }
    #[test]
    fn top_winding_and_uvs_have_known_world_orientation() {
        let q = bake(&model(), &variant()).unwrap().remove(0);
        assert_eq!(
            q.positions,
            [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]]
        );
        assert_eq!(q.uvs, [[0., 0.], [0., 1.], [1., 1.], [1., 0.]]);
        assert_eq!(q.facing, Face::Up);
        assert_eq!(q.tint_index, 0);
        assert_eq!(q.cull_face, Some(Face::Up));
    }
    #[test]
    fn uvlock_keeps_world_aligned_top_texture_after_rotation() {
        let mut variant = variant();
        variant.rotations = [0, 90, 0];
        variant.uv_lock = true;
        let q = bake(&model(), &variant).unwrap().remove(0);
        for (position, uv) in q.positions.into_iter().zip(q.uvs) {
            close(uv, [position[0], position[2]]);
        }
        variant.uv_lock = false;
        let unlocked = bake(&model(), &variant).unwrap().remove(0);
        close(unlocked.uvs[0], [0., 0.]);
        assert!((unlocked.positions[0][0] - 1.).abs() < 0.00001);
    }
    #[test]
    fn z_rotation_rotates_geometry_and_cull_direction() {
        let mut variant = variant();
        variant.rotations = [0, 0, 90];
        let q = bake(&model(), &variant).unwrap().remove(0);
        assert_eq!(q.facing, Face::East);
        assert_eq!(q.cull_face, Some(Face::East));
        assert!(q.positions.iter().all(|p| (p[0] - 1.).abs() < 0.00001));
    }
}
