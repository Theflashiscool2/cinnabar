//! Ground items reuse native item models and the existing actor atlas/rig owner.
//! Dry movement is sampled from the native item view; pickups remain server-owned.
use super::*;
use render_model::{ActorGlint, HandItemAlphaMode};

/// Immutable render facts shared by a live item and its authoritative pickup snapshot.
#[derive(Clone, Copy)]
pub(super) struct Source {
    pub identity: Identity,
    pub uuid: [u8; 16],
    pub revision: u64,
}
impl From<&world::java::entities::Entity> for Source {
    fn from(entity: &world::java::entities::Entity) -> Self {
        Self {
            identity: entity.identity,
            uuid: entity.uuid,
            revision: entity.revision,
        }
    }
}
pub(super) struct Frame {
    pub scope: (u64, u64),
    pub serial: u64,
    pub age: f32,
    pub light: u32,
    pub position: [f64; 3],
}
/// Native ItemClusterRenderState thresholds, including its unused empty-stack branch.
fn amount(count: u32) -> usize {
    match count {
        0..=1 => 1,
        2..=16 => 2,
        17..=32 => 3,
        33..=48 => 4,
        _ => 5,
    }
}
/// Native LegacyRandomSource.setSeed/nextFloat; offsets are stable across frames/reloads.
struct Random(u64);
impl Random {
    fn new(seed: i64) -> Self {
        Self((seed as u64 ^ 0x5deece66d) & ((1 << 48) - 1))
    }
    fn next(&mut self) -> f32 {
        self.0 = (self.0.wrapping_mul(0x5deece66d).wrapping_add(11)) & ((1 << 48) - 1);
        ((self.0 >> 24) as f32) / 16777216.
    }
}
fn offsets(count: u32, depth: f32, seed: i64) -> Vec<[f32; 3]> {
    let count = amount(count);
    let mut random = Random::new(seed);
    let thick = depth > 0.0625;
    let step = depth * 1.5;
    (0..count)
        .map(|i| {
            if thick {
                if i == 0 {
                    [0.; 3]
                } else {
                    std::array::from_fn(|_| (random.next() * 2. - 1.) * 0.15)
                }
            } else {
                let z = step * (i as f32 - (count - 1) as f32 / 2.);
                if i == 0 {
                    [0., 0., z]
                } else {
                    [
                        (random.next() * 2. - 1.) * 0.15 * 0.5,
                        (random.next() * 2. - 1.) * 0.15 * 0.5,
                        z,
                    ]
                }
            }
        })
        .collect()
}
fn pose(age: f32, phase: f32, bounds: ::assets::SkinGeometryBounds, position: Vec3) -> Mat4 {
    let bob = (age / 10. + phase).sin() * 0.1 + 0.1;
    let hover = -(bounds.center[1] - bounds.half_extents[1]) + 0.0625;
    Mat4::from_translation(position + Vec3::Y * (bob + hover))
        * Mat4::from_rotation_y(age / 20. + phase)
}
pub(super) fn append(
    batch: &mut ActorPresentationBatch,
    entity: Source,
    entry: &equipment::Entry,
    count: u32,
    frame: Frame,
) -> Result<usize, String> {
    let Some(first) = entry.draws.first() else {
        return Ok(0);
    };
    // Native phase is a client-local random float, never an authority/simulation input.
    let phase = ((u32::from_be_bytes(entity.uuid[..4].try_into().unwrap()) >> 8) as f32
        / 16777216.)
        * std::f32::consts::TAU;
    let root = pose(
        frame.age,
        phase,
        first.bounds,
        Vec3::from_array(frame.position.map(|n| n as f32)),
    );
    let bones: Arc<[RenderBoneTransform]> = Arc::from([RenderBoneTransform {
        rotation: [0., 0., 0., 1.],
        translation_scale: [0., 0., 0., 1.],
        axis_scale: [1.; 4],
    }]);
    let revision = std::num::NonZeroU64::new(entity.identity.incarnation)
        .ok_or("native item incarnation missing")?;
    let copies = offsets(count, first.bounds.half_extents[2] * 2., entry.seed);
    for (copy, offset) in copies.iter().enumerate() {
        let matrix = (root * Mat4::from_translation(Vec3::from_array(*offset))).to_cols_array_2d();
        for (index, draw) in entry.draws.iter().enumerate() {
            let layer = copy * entry.draws.len() + index;
            if layer >= usize::from(render_model::ACTOR_LAYER_SKIN_BASE) {
                return Err("native ground item layer budget exhausted".into());
            }
            let identity = ActorRenderIdentity {
                session_id: frame.scope.0,
                dimension: 0,
                runtime_id: entity.identity.incarnation,
                spawn_revision: revision,
                ingress_sequence: entity.revision,
                source_tick: None,
                movement_revision: entity.revision,
                pose_generation: render_api::PoseGeneration::new(frame.serial),
                layer: layer as u8,
            };
            let mut material = ActorMaterial::default();
            material.state = Some(::assets::EntityRenderMaterialState {
                alpha_test: draw.alpha != HandItemAlphaMode::Opaque,
                cull: true,
                blend: draw.alpha == HandItemAlphaMode::Blend,
                depth_write: draw.alpha != HandItemAlphaMode::Blend,
                ..Default::default()
            });
            if draw.glint {
                material.kind = ::assets::EntityRenderMaterial::Glint;
                material.glint = ActorGlint::item(frame.age / 20., [1.; 2]);
            }
            batch.artwork.insert(identity, draw.artwork);
            batch.submissions.push(ActorRigSubmission {
                material,
                culling_bounds: draw.bounds,
                input: ActorRigRenderInput {
                    identity,
                    rig: draw.rig,
                    previous_bones: bones.clone(),
                    current_bones: bones.clone(),
                    completed_tick: frame.serial,
                    reset_generation: frame.scope.1,
                },
                world_from_actor: std::array::from_fn(|r| {
                    [matrix[0][r], matrix[1][r], matrix[2][r], matrix[3][r]]
                }),
                texture_layer: draw.artwork.layer,
                route: ActorRigRoute::Compiled,
                tint: 0,
                overlay_rgba8: 0,
                uv_anim: [0., 0., 1., 1.],
                light: frame.light,
            });
        }
    }
    Ok(copies.len() * entry.draws.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_stack_thresholds_and_flat_copy_spacing() {
        assert_eq!(
            [0, 1, 2, 16, 17, 32, 33, 48, 49, 64].map(amount),
            [1, 1, 2, 2, 3, 3, 4, 4, 5, 5]
        );
        let flat = offsets(17, 0.03125, 0);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0], [0., 0., -0.046875]);
        assert_eq!(flat[1][2], 0.);
        assert_eq!(flat[2][2], 0.046875);
        assert_eq!(offsets(1, 0.25, 0), [[0.; 3]]);
        assert_eq!(flat, offsets(17, 0.03125, 0));
    }
    #[test]
    fn native_random_vectors_and_hover_spin_use_model_bounds() {
        // Native RandomSource.create().setSeed(0) reference, not values from this implementation.
        let mut random = Random::new(0);
        let expected = [
            0x3f3b20b4, 0x3f54d951, 0x3e764f2c, 0x3f1b3970, 0x3f232dc9, 0x3e9e3be0,
        ];
        assert_eq!(
            std::array::from_fn::<_, 6, _>(|_| random.next().to_bits()),
            expected
        );
        let bounds = ::assets::SkinGeometryBounds {
            center: [0.; 3],
            half_extents: [0.125; 3],
        };
        let m = pose(0., 0., bounds, Vec3::new(4., 5., 6.));
        assert!((m.transform_point3(Vec3::new(0., -0.125, 0.)).y - 5.1625).abs() < 1e-6);
        let rotated = pose(20., 0., bounds, Vec3::ZERO).transform_vector3(Vec3::X);
        assert!((rotated.x - 0.5403023).abs() < 1e-6 && (rotated.z + 0.84147096).abs() < 1e-6);
    }
}
