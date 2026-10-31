//! Native Java Cube texture layout, shared by special block models.
use super::{
    atlas::TextureRef,
    mesh::{Layer, MeshBuilder, WHITE},
};
use assets::block_entity_geometry::BoxSpec;
use glam::{Mat4, Vec3};

/// Java Cube UV unwrap differs from Bedrock even when the box coordinates coincide.
/// Checked against the pinned client's ModelPart.Cube polygon positions and texels.
pub(super) fn faces(spec: BoxSpec) -> [assets::block_entity_geometry::ModelFace; 6] {
    let [x, y, z] = spec.size;
    let [u, v] = spec.uv;
    let rects = [
        [u + z + x, v + z + y, -x, -y],
        [u + 2. * z + 2. * x, v + z + y, -x, -y],
        [u + 2. * z + x, v + z + y, -z, -y],
        [u + z, v + z + y, -z, -y],
        [u + z + 2. * x, v, -x, z],
        [u + z + x, v + z, -x, -z],
    ];
    let mut faces = spec.faces();
    for (face, uv) in faces.iter_mut().zip(rects) {
        face.1 = uv;
    }
    faces
}
pub(super) fn cuboid(
    builder: &mut MeshBuilder,
    texture: &TextureRef,
    model: Mat4,
    spec: BoxSpec,
    mask: u8,
) {
    for (index, (corners, texels)) in faces(spec).into_iter().enumerate() {
        if mask & (1 << index) == 0 {
            continue;
        }
        builder.quad(
            Layer::Solid,
            corners.map(|p| model.transform_point3(Vec3::from_array(p)).to_array()),
            texture.rect_uv(texels),
            WHITE,
        );
    }
}
