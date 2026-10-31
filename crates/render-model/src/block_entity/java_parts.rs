//! Native exported model hierarchy emission into the shared special pass.
use super::mesh::{Layer, MeshBuilder, WHITE};
use assets::entity::java::Part;
use glam::{EulerRot, Mat4, Quat, Vec3};
pub(super) fn part(
    builder: &mut MeshBuilder,
    part: &Part,
    parent: Mat4,
    texture: super::atlas::TextureRef,
    override_pose: Option<Mat4>,
) {
    if !part.visible {
        return;
    }
    let [x, y, z] = part.rotation;
    let pose = parent
        * override_pose.unwrap_or_else(|| {
            Mat4::from_scale_rotation_translation(
                Vec3::from_array(part.scale),
                Quat::from_euler(EulerRot::ZYX, z, y, x),
                Vec3::from_array(part.offset),
            )
        });
    if !part.skip_draw {
        for face in &part.faces {
            // Java exported faces are CCW about their normals. The shared special pass expects
            // the reverse order and derives an outward normal from cross(v2-v0,v1-v0).
            let mut points = face.vertices.map(|v| {
                pose.transform_point3(Vec3::new(v[0], v[1], v[2]))
                    .to_array()
            });
            let mut uv = face.vertices.map(|v| {
                [
                    texture.rect.x + v[3] * texture.rect.width,
                    texture.rect.y + v[4] * texture.rect.height,
                ]
            });
            // Native zero-thickness cubes export collapsed edge faces. They rasterize nothing;
            // omit those zero-area quads rather than publishing vertices with a zero normal.
            let origin = Vec3::from_array(points[0]);
            if (Vec3::from_array(points[1]) - origin)
                .cross(Vec3::from_array(points[2]) - origin)
                .length_squared()
                < 1e-16
            {
                continue;
            }
            points.reverse();
            uv.reverse();
            builder.quad_uv(Layer::Solid, points, uv, WHITE);
        }
    }
    for child in &part.children {
        self::part(builder, child, pose, texture, None);
    }
}
