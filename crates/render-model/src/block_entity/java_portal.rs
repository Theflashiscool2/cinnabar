//! Native Java portal surfaces and screen-projected GPU parameters.
use super::{
    atlas::BlockEntityAtlas,
    mesh::{Layer, MeshBuilder},
    scene::{BlockEntityKind, BlockEntitySubmission},
};
use assets::registry::java::portal::{LAYERS, Resources, SKY, STARS};
use std::sync::Arc;
/// Two texture rectangles and the archive's sixteen RGB palette values.
pub const PARAMETER_WORDS: usize = 18;
pub type Parameters = [[f32; 4]; PARAMETER_WORDS];
#[derive(Clone, Debug)]
pub struct Model {
    pub resources: Arc<Resources>,
}
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.resources, &other.resources)
    }
}
pub(super) fn parameters(
    atlas: &BlockEntityAtlas,
    submissions: &[BlockEntitySubmission],
) -> Parameters {
    let mut result = [[0.; 4]; PARAMETER_WORDS];
    let Some(model) = submissions.iter().find_map(|s| match &s.kind {
        BlockEntityKind::JavaPortal(m) => Some(&m.resources),
        BlockEntityKind::JavaGateway(m) => Some(&m.resources),
        _ => None,
    }) else {
        return result;
    };
    let [w, h] = atlas.size().map(|v| v as f32);
    for (i, name) in [SKY, STARS].iter().enumerate() {
        let Some(t) = atlas.texture(name, [1.; 2]) else {
            return [[0.; 4]; PARAMETER_WORDS];
        };
        result[i] = [
            t.rect.x / w,
            t.rect.y / h,
            t.rect.width / w,
            t.rect.height / h,
        ];
    }
    result[2..].copy_from_slice(&model.colors);
    result
}
pub(super) fn emit(builder: &mut MeshBuilder, block: [i32; 3]) {
    let [x, y, z] = block.map(|v| v as f32);
    // TheEndPortalBlockEntity exposes only +/-Y; renderer transforms cube to [3/8,3/4].
    for corners in [
        [
            [x, y + 0.75, z + 1.],
            [x + 1., y + 0.75, z + 1.],
            [x + 1., y + 0.75, z],
            [x, y + 0.75, z],
        ],
        [
            [x, y + 0.375, z],
            [x + 1., y + 0.375, z],
            [x + 1., y + 0.375, z + 1.],
            [x, y + 0.375, z + 1.],
        ],
    ] {
        // Negative phase explicitly selects Java projection; Bedrock encoding remains untouched.
        builder.quad_uv(
            Layer::Portal,
            corners,
            [[0.; 2]; 4],
            [0., 0., 0., -(LAYERS as f32)],
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_native_faces_heights_winding_and_translation() {
        let mut b = MeshBuilder::new([1; 2]);
        emit(&mut b, [-17, -5, 16]);
        assert_eq!(b.portal.len(), 12);
        for (i, tri) in b.portal.chunks_exact(3).enumerate() {
            let v = tri
                .iter()
                .map(|v| glam::Vec3::from(v.position))
                .collect::<Vec<_>>();
            let expected = if i < 2 { glam::Vec3::Y } else { -glam::Vec3::Y };
            assert!((v[1] - v[0]).cross(v[2] - v[0]).dot(expected) > 0.);
            for vertex in tri {
                assert_eq!(vertex.position[1], if i < 2 { -4.25 } else { -4.625 });
                assert_eq!(vertex.color[3], -15.);
            }
        }
        let mut origin = MeshBuilder::new([1; 2]);
        emit(&mut origin, [0; 3]);
        for (a, b) in origin.portal.iter().zip(&b.portal) {
            for k in 0..3 {
                assert_eq!(b.position[k] - [-17., -5., 16.][k], a.position[k]);
            }
        }
    }
    #[test]
    fn atlas_revision_and_edition_changes_do_not_retain_projector_data() {
        use super::super::scene::BlockEntityLight;
        use assets::{BlockEntityPlacement, RuntimeBlockEntityAssets, encode_block_entity_catalog};
        let mut placements = [
            BlockEntityPlacement {
                name: SKY.into(),
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            BlockEntityPlacement {
                name: STARS.into(),
                x: 2,
                y: 0,
                width: 2,
                height: 2,
            },
        ];
        placements.sort_by(|a, b| a.name.cmp(&b.name));
        let bytes = encode_block_entity_catalog(b"{}", 4, 2, &[255; 32], &placements).unwrap();
        let atlas =
            BlockEntityAtlas::from_assets(&RuntimeBlockEntityAssets::decode(&bytes).unwrap());
        let submission = |color| BlockEntitySubmission {
            block: [0; 3],
            light: BlockEntityLight::Scalar(1.),
            kind: BlockEntityKind::JavaPortal(Model {
                resources: Arc::new(Resources {
                    colors: [color; 16],
                }),
            }),
        };
        let first = parameters(&atlas, &[submission([0.1, 0.2, 0.3, 0.])]);
        assert_eq!(first[0], [0., 0., 0.5, 2. / atlas.size()[1] as f32]);
        assert_eq!(first[1], [0.5, 0., 0.5, 2. / atlas.size()[1] as f32]);
        assert_eq!(first[2], [0.1, 0.2, 0.3, 0.]);
        let replacement = parameters(&atlas, &[submission([0.4, 0.5, 0.6, 0.])]);
        assert_eq!(replacement[2], [0.4, 0.5, 0.6, 0.]);
        assert_eq!(parameters(&atlas, &[]), [[0.; 4]; PARAMETER_WORDS]);
        assert_eq!(
            parameters(
                &atlas,
                &[BlockEntitySubmission {
                    block: [0; 3],
                    light: BlockEntityLight::Scalar(1.),
                    kind: BlockEntityKind::EndPortal
                }]
            ),
            [[0.; 4]; PARAMETER_WORDS]
        );
    }
}
