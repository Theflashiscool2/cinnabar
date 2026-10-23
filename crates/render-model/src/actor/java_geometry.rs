//! Native Java model admission into the existing actor rig contract.
//! Input coordinates remain Java pixels; bones own the conversion to Cinnabar actor space.
use super::{
    ActorRigGeometry, ActorRigGeometryError, ActorRigSurface, ActorRigVertex, EntityRigId,
    MAX_RENDER_BONES_PER_ACTOR, RenderBoneTransform,
};
use assets::entity::java::Part;
use glam::{Affine3A, EulerRot, Mat3, Quat, Vec3};
use std::sync::Arc;

#[derive(Debug)]
pub enum Error {
    Geometry(ActorRigGeometryError),
    Transform,
    BoneCount,
    MissingGeometry,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid native Java rig: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<ActorRigGeometryError> for Error {
    fn from(e: ActorRigGeometryError) -> Self {
        Self::Geometry(e)
    }
}
#[derive(Clone, Debug)]
struct Bone {
    path: String,
    parent: Option<usize>,
    offset: Vec3,
    rotation: [f32; 3],
    scale: Vec3,
}
#[derive(Clone, Debug)]
pub struct Model {
    pub geometry: ActorRigGeometry,
    bones: Vec<Bone>,
    pub rest_pose: Arc<[RenderBoneTransform]>,
    pub bounds: assets::SkinGeometryBounds,
}
impl Model {
    /// Runs on preparation workers. Pixel data and packet schemas never enter this conversion.
    pub fn compile(root: &Part, id: EntityRigId) -> Result<Self, Error> {
        let mut vertices = Vec::new();
        let mut bones = Vec::new();
        collect(root, None, "", true, &mut bones, &mut vertices)?;
        if vertices.is_empty() {
            return Err(Error::MissingGeometry);
        }
        let geometry = ActorRigGeometry::new(id, vertices, vec![[0.; 3]; bones.len()])?;
        let rest_pose = pose(&bones, &[])?;
        let matrices = rest_pose.iter().map(affine).collect::<Vec<_>>();
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for v in geometry.vertices.iter() {
            let p = matrices[v.bone_index as usize].transform_point3(Vec3::from_array(v.position));
            low = low.min(p);
            high = high.max(p);
        }
        let bounds = assets::SkinGeometryBounds {
            center: ((low + high) * 0.5).to_array(),
            half_extents: ((high - low) * 0.5).to_array(),
        };
        Ok(Self {
            geometry,
            bones,
            rest_pose,
            bounds,
        })
    }
    /// Uses cached per-bone vertex bounds, so animated culling contains head and limb rotations.
    pub fn posed_bounds(
        &self,
        pose: &[RenderBoneTransform],
    ) -> Result<assets::SkinGeometryBounds, Error> {
        if pose.len() < self.bones.len() {
            return Err(Error::BoneCount);
        }
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for (i, t) in pose.iter().enumerate().take(self.bones.len()) {
            if !t.is_finite() {
                return Err(Error::Transform);
            }
            let Some((min, max)) = self.geometry.bone_bounds(i) else {
                continue;
            };
            let matrix = affine(t);
            for bits in 0..8 {
                let point = Vec3::from_array(std::array::from_fn(|axis| {
                    if bits & (1 << axis) == 0 {
                        min[axis]
                    } else {
                        max[axis]
                    }
                }));
                let point = matrix.transform_point3(point);
                if !point.is_finite() {
                    return Err(Error::Transform);
                }
                low = low.min(point);
                high = high.max(point);
            }
        }
        if !low.is_finite() || !high.is_finite() {
            return Err(Error::MissingGeometry);
        }
        Ok(assets::SkinGeometryBounds {
            center: ((low + high) * 0.5).to_array(),
            half_extents: ((high - low) * 0.5).to_array(),
        })
    }
    pub fn bone(&self, path: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.path == path)
    }
    /// Replaces native local Euler rotations, after the target's reset-to-model-pose step.
    /// The renderer consumes only ordinary rigid/scaled bone transforms, never Java packet data.
    pub fn pose(
        &self,
        rotations: &[(usize, [f32; 3])],
    ) -> Result<Arc<[RenderBoneTransform]>, Error> {
        pose(&self.bones, rotations)
    }

    /// Hides native parts and their descendants without rebuilding geometry or introducing
    /// singular rotations. Zero uniform scale collapses their triangles in the shared rig.
    pub fn pose_with_visibility(
        &self,
        rotations: &[(usize, [f32; 3])],
        hidden: &[usize],
    ) -> Result<Arc<[RenderBoneTransform]>, Error> {
        if hidden.iter().any(|index| *index >= self.bones.len()) {
            return Err(Error::BoneCount);
        }
        let mut transforms = self.pose(rotations)?;
        let output = Arc::make_mut(&mut transforms);
        let mut invisible = vec![false; self.bones.len()];
        for (i, bone) in self.bones.iter().enumerate() {
            invisible[i] = hidden.contains(&i) || bone.parent.is_some_and(|p| invisible[p]);
            if invisible[i] {
                output[i].translation_scale[3] = 0.;
            }
        }
        Ok(transforms)
    }
}

fn collect(
    part: &Part,
    parent: Option<usize>,
    prefix: &str,
    visible: bool,
    bones: &mut Vec<Bone>,
    vertices: &mut Vec<ActorRigVertex>,
) -> Result<(), Error> {
    if bones.len() >= MAX_RENDER_BONES_PER_ACTOR {
        return Err(Error::BoneCount);
    }
    let path = if prefix.is_empty() {
        part.name.clone()
    } else {
        format!("{prefix}/{}", part.name)
    };
    let index = bones.len();
    bones.push(Bone {
        path: path.clone(),
        parent,
        offset: Vec3::from_array(part.offset) / 16.,
        rotation: part.rotation,
        scale: Vec3::from_array(part.scale),
    });
    let visible = visible && part.visible;
    if visible && !part.skip_draw {
        for face in &part.faces {
            // The runtime's emitted quad winding is counterclockwise about the exported normal.
            for i in [0, 1, 2, 0, 2, 3] {
                let [x, y, z, u, v] = face.vertices[i];
                vertices.push(ActorRigVertex {
                    position: [x / 16., y / 16., z / 16.],
                    normal: face.normal,
                    uv: [u, v],
                    back_uv: super::ONE_SIDED_BACK_UV,
                    bone_index: index as u32,
                    surface: ActorRigSurface::SINGLE_FACE,
                    glint_origin: [0.; 2],
                });
            }
        }
    }
    for child in &part.children {
        collect(child, Some(index), &path, visible, bones, vertices)?;
    }
    Ok(())
}
fn pose(
    bones: &[Bone],
    rotations: &[(usize, [f32; 3])],
) -> Result<Arc<[RenderBoneTransform]>, Error> {
    if rotations
        .iter()
        .any(|(i, r)| *i >= bones.len() || !r.iter().all(|v| v.is_finite()))
    {
        return Err(Error::Transform);
    }
    // Pinned LivingEntityRenderer: Y(180-bodyYaw), scale(-1,-1,1), translate(0,-1.501,0).
    // Body yaw belongs in world_from_actor. At yaw zero this is an X half-turn and +Y offset.
    let base = Affine3A::from_rotation_translation(
        Quat::from_xyzw(1., 0., 0., 0.),
        Vec3::new(0., 1.501, 0.),
    );
    let mut matrices: Vec<Affine3A> = Vec::with_capacity(bones.len());
    let mut out = Vec::with_capacity(bones.len());
    for (i, bone) in bones.iter().enumerate() {
        let [x, y, z] = rotations
            .iter()
            .rev()
            .find(|(index, _)| *index == i)
            .map_or(bone.rotation, |(_, r)| *r);
        let local = Affine3A::from_scale_rotation_translation(
            bone.scale,
            Quat::from_euler(EulerRot::ZYX, z, y, x),
            bone.offset,
        );
        let global = bone.parent.map_or(base, |p| matrices[p]) * local;
        let (scale, rotation, translation) = global.to_scale_rotation_translation();
        let transform = RenderBoneTransform {
            rotation: rotation.to_array(),
            translation_scale: [translation.x, translation.y, translation.z, 1.],
            axis_scale: [scale.x, scale.y, scale.z, 1.],
        };
        if !transform.is_finite() {
            return Err(Error::Transform);
        }
        // The shared bone contract cannot represent shear. Reject it instead of distorting a model.
        let rebuilt = affine(&transform);
        if global
            .to_cols_array()
            .iter()
            .zip(rebuilt.to_cols_array())
            .any(|(a, b)| (*a - b).abs() > 1e-5)
        {
            return Err(Error::Transform);
        }
        matrices.push(global);
        out.push(transform);
    }
    Ok(out.into())
}
fn affine(t: &RenderBoneTransform) -> Affine3A {
    let q = Quat::from_array(t.rotation).normalize();
    let scale = Vec3::from_array([t.axis_scale[0], t.axis_scale[1], t.axis_scale[2]])
        * t.translation_scale[3];
    Affine3A::from_mat3_translation(
        Mat3::from_quat(q) * Mat3::from_diagonal(scale),
        Vec3::new(
            t.translation_scale[0],
            t.translation_scale[1],
            t.translation_scale[2],
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn part(
        name: &str,
        offset: [f32; 3],
        faces: Vec<assets::entity::java::Face>,
        children: Vec<Part>,
    ) -> Part {
        Part {
            name: name.into(),
            offset,
            rotation: [0.; 3],
            scale: [1.; 3],
            visible: true,
            skip_draw: false,
            faces,
            children,
        }
    }
    fn face() -> assets::entity::java::Face {
        assets::entity::java::Face {
            normal: [0., 0., -1.],
            vertices: [
                [-4., -4., -8., 0., 0.],
                [-4., 4., -8., 0., 1.],
                [4., 4., -8., 1., 1.],
                [4., -4., -8., 1., 0.],
            ],
        }
    }
    #[test]
    fn native_part_visibility_hides_descendants_and_restores_without_mutating_rest_pose() {
        let root = part(
            "root",
            [0.; 3],
            vec![face()],
            vec![part(
                "hat",
                [0., 16., 0.],
                vec![face()],
                vec![part("detail", [0., 1., 0.], vec![face()], vec![])],
            )],
        );
        let model = Model::compile(&root, EntityRigId(123)).unwrap();
        let hat = model.bone("root/hat").unwrap();
        let detail = model.bone("root/hat/detail").unwrap();
        let hidden = model.pose_with_visibility(&[], &[hat]).unwrap();
        assert_eq!(hidden[0].translation_scale[3], 1.);
        for index in [hat, detail] {
            assert!(hidden[index].is_finite());
            assert_eq!(hidden[index].translation_scale[3], 0.);
            let matrix = affine(&hidden[index]);
            assert_eq!(
                matrix.transform_point3(Vec3::ZERO),
                matrix.transform_point3(Vec3::ONE)
            );
        }
        assert!(model.posed_bounds(&hidden).is_ok());
        assert!(
            model
                .rest_pose
                .iter()
                .all(|bone| bone.translation_scale[3] == 1.)
        );
        assert!(
            model
                .pose_with_visibility(&[], &[])
                .unwrap()
                .iter()
                .all(|bone| bone.translation_scale[3] == 1.)
        );
        assert!(model.pose_with_visibility(&[], &[usize::MAX]).is_err());
    }
    #[test]
    fn coordinates_and_head_rotations_match_independent_native_pose_stack_vectors() {
        let root = part(
            "root",
            [0.; 3],
            vec![],
            vec![part("head", [0., 12., -6.], vec![face()], vec![])],
        );
        let model = Model::compile(&root, EntityRigId(123)).unwrap();
        let head = model.bone("root/head").unwrap();
        // Evaluated with the pinned client PigModel and PoseStack, not the Rust conversion.
        for (yaw, pitch, expected) in [
            (0f32, 0f32, [-0.25, 1.001, 0.875]),
            (0., 30., [-0.24999999, 0.7175064, 0.9330127]),
            (90., 0., [-0.49999997, 1.001, 0.12500001]),
            (90., 30., [-0.55801266, 0.7175064, 0.12500003]),
        ] {
            let pose = model
                .pose(&[(head, [pitch.to_radians(), yaw.to_radians(), 0.])])
                .unwrap();
            let point =
                affine(&pose[head]).transform_point3(Vec3::new(-4. / 16., -4. / 16., -8. / 16.));
            for (a, b) in point.to_array().into_iter().zip(expected) {
                assert!((a - b).abs() < 5e-7, "{yaw} {pitch}: {point:?}");
            }
            let bounds = model.posed_bounds(&pose).unwrap();
            for (axis, expected) in expected.into_iter().enumerate() {
                assert!(
                    (expected - bounds.center[axis]).abs() <= bounds.half_extents[axis] + 5e-7,
                    "rotated head escaped its culling bounds"
                );
            }
        }
        assert_eq!(model.geometry.vertices.len(), 6);
        assert_eq!(model.geometry.vertices[0].uv, [0., 0.]);
        assert!(model.pose(&[(head, [f32::NAN, 0., 0.])]).is_err());
        assert!(model.pose(&[(usize::MAX, [0.; 3])]).is_err());
    }
    #[test]
    fn child_transform_and_visibility_are_not_flattened_into_bedrock_bones() {
        let mut branch = part(
            "branch",
            [16., 0., 0.],
            vec![],
            vec![part("leaf", [0., 16., 0.], vec![face()], vec![])],
        );
        branch.rotation[2] = std::f32::consts::FRAC_PI_2;
        let root = part("root", [0.; 3], vec![], vec![branch.clone()]);
        let model = Model::compile(&root, EntityRigId(123)).unwrap();
        let leaf = model.bone("root/branch/leaf").unwrap();
        let origin = affine(&model.rest_pose[leaf]).transform_point3(Vec3::ZERO);
        assert!((origin - Vec3::new(0., 1.501, 0.)).length() < 1e-6);
        branch.visible = false;
        assert!(matches!(
            Model::compile(
                &part("root", [0.; 3], vec![], vec![branch]),
                EntityRigId(123)
            ),
            Err(Error::MissingGeometry)
        ));
    }
}
