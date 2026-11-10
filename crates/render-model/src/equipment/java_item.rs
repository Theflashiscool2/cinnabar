//! Native item quads become immutable material groups for the existing first-person GPU pass.
use crate::{ActorRigGeometry, ActorRigSurface, ActorRigVertex, HandItemAtlas};
use assets::registry::java::items::Quad;
use glam::Vec3;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
mod attachment;
pub use attachment::{held_bone, posed_bounds};
/// One geometry per sampled material; each uses the corresponding texture-array layer.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub groups: Vec<ActorRigGeometry>,
    pub alpha_modes: Vec<crate::HandItemAlphaMode>,
    pub atlas: HandItemAtlas,
    pub bounds: assets::SkinGeometryBounds,
}
/// Prepares actual model quads, never an extruded GUI thumbnail. Runs on artwork workers.
/// Operational texture/geometry budgets fail explicitly rather than substituting artwork.
pub fn prepare(quads: Vec<Quad>, offhand: bool) -> Result<Option<Prepared>, String> {
    prepare_with_ids(quads, |index| {
        crate::actor::java_model_rig_id(
            (index + usize::from(offhand) * crate::hand::MAX_HAND_ITEM_LAYERS) as u32,
        )
    })
}
/// The same material/geometry preparation with a caller-owned world-rig namespace.
pub fn prepare_with_ids(
    quads: Vec<Quad>,
    mut rig: impl FnMut(usize) -> Option<crate::EntityRigId>,
) -> Result<Option<Prepared>, String> {
    if quads.is_empty() {
        return Ok(None);
    }
    let mut keys = BTreeMap::new();
    let mut materials = Vec::new();
    let mut faces: Vec<Vec<Quad>> = Vec::new();
    for quad in quads {
        let image = &quad.image;
        if image.width == 0
            || image.height == 0
            || image.width > assets::texture::policy::MAX_TEXTURE_SIDE
            || image.height > assets::texture::policy::MAX_TEXTURE_SIDE
            || (image.width as usize)
                .checked_mul(image.height as usize)
                .and_then(|n| n.checked_mul(4))
                != Some(image.rgba.len())
        {
            return Err("invalid native held material image".into());
        }
        let mut hash = Sha256::new();
        hash.update(image.width.to_le_bytes());
        hash.update(image.height.to_le_bytes());
        hash.update(&image.rgba);
        let key: ([u8; 32], [u32; 4]) = (hash.finalize().into(), quad.color.map(f32::to_bits));
        let index = if let Some(index) = keys.get(&key) {
            *index
        } else {
            let index = materials.len();
            if index >= crate::hand::MAX_HAND_ITEM_LAYERS {
                return Err("native held material-array budget exhausted".into());
            }
            keys.insert(key, index);
            materials.push((quad.image.clone(), quad.color));
            faces.push(Vec::new());
            index
        };
        faces[index].push(quad);
    }
    let width = materials.iter().map(|(i, _)| i.width).max().unwrap();
    let height = materials.iter().map(|(i, _)| i.height).max().unwrap();
    let layer_bytes = width as usize * height as usize * 4;
    let bytes = layer_bytes
        .checked_mul(materials.len())
        .filter(|n| *n <= assets::texture::policy::MAX_SUBSCRIBER_BYTES)
        .ok_or("native held material storage budget exhausted")?;
    let mut rgba = vec![0; bytes];
    let alpha_modes = materials
        .iter()
        .map(|(image, color)| {
            let mut opaque = true;
            let mut binary = true;
            for pixel in image.rgba.chunks_exact(4) {
                let alpha = (pixel[3] as f32 * color[3]).round().clamp(0., 255.) as u8;
                opaque &= alpha == 255;
                binary &= alpha == 0 || alpha == 255;
            }
            if opaque {
                crate::HandItemAlphaMode::Opaque
            } else if binary {
                crate::HandItemAlphaMode::Cutout
            } else {
                crate::HandItemAlphaMode::Blend
            }
        })
        .collect();
    for (layer, (image, color)) in materials.iter().enumerate() {
        for y in 0..image.height as usize {
            for x in 0..image.width as usize {
                for channel in 0..4 {
                    let value = image.rgba[(y * image.width as usize + x) * 4 + channel] as f32
                        * color[channel];
                    rgba[layer * layer_bytes + (y * width as usize + x) * 4 + channel] =
                        value.round().clamp(0., 255.) as u8;
                }
            }
        }
    }
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    let mut groups = Vec::new();
    for (index, faces) in faces.into_iter().enumerate() {
        let mut vertices = Vec::new();
        for quad in faces {
            let p = quad.positions.map(Vec3::from_array);
            let normal = quad.normal.unwrap_or_else(|| {
                (p[1] - p[0])
                    .cross(p[2] - p[0])
                    .normalize_or_zero()
                    .to_array()
            });
            for v in &p {
                low = low.min(*v);
                high = high.max(*v);
            }
            for corner in [0, 1, 2, 0, 2, 3] {
                let uv = [
                    quad.uv[corner][0] * quad.image.width as f32 / width as f32,
                    quad.uv[corner][1] * quad.image.height as f32 / height as f32,
                ];
                vertices.push(ActorRigVertex {
                    position: quad.positions[corner],
                    normal,
                    uv,
                    back_uv: if quad.cull_back {
                        crate::ONE_SIDED_BACK_UV
                    } else {
                        uv
                    },
                    bone_index: 0,
                    surface: if quad.cull_back {
                        ActorRigSurface::SINGLE_FACE
                    } else {
                        ActorRigSurface::OPPOSING_FACES
                    },
                    glint_origin: [0.; 2],
                });
            }
        }
        let id = rig(index).ok_or("native held geometry identity exhausted")?;
        groups.push(
            ActorRigGeometry::new(id, vertices, vec![[0.; 3]])
                .map_err(|e| format!("native held geometry: {e:?}"))?,
        );
    }
    Ok(Some(Prepared {
        groups,
        alpha_modes,
        atlas: HandItemAtlas {
            width: width as u16,
            height: height as u16,
            layers: materials.len() as u32,
            rgba8: rgba.into(),
        },
        bounds: assets::SkinGeometryBounds {
            center: ((low + high) * 0.5).to_array(),
            half_extents: ((high - low) * 0.5).to_array(),
        },
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    use assets::registry::java::textures::Image;
    use std::sync::Arc;
    fn face(image: Arc<Image>, color: [f32; 4]) -> Quad {
        Quad {
            positions: [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
            uv: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
            image,
            color,
            block_light: true,
            cull_back: true,
            normal: None,
        }
    }
    #[test]
    fn groups_real_materials_and_keeps_texture_coordinates_and_tint() {
        let image = Arc::new(Image {
            width: 2,
            height: 2,
            rgba: vec![100, 200, 40, 128].repeat(4),
        });
        let small = Arc::new(Image {
            width: 1,
            height: 1,
            rgba: vec![200, 50, 100, 255],
        });
        let prepared = prepare(
            vec![
                face(image.clone(), [1.; 4]),
                face(image, [1.; 4]),
                face(small, [0.5, 1., 1., 1.]),
            ],
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(prepared.groups.len(), 2);
        assert_eq!(
            prepared.alpha_modes,
            [
                crate::HandItemAlphaMode::Blend,
                crate::HandItemAlphaMode::Opaque
            ]
        );
        assert_eq!(prepared.groups[0].vertices.len(), 12);
        assert_eq!(
            prepared.atlas.rgba8.range(0, 4).as_ref(),
            &[100, 200, 40, 128]
        );
        assert_eq!(
            prepared.atlas.rgba8.range(16, 4).as_ref(),
            &[100, 50, 100, 255]
        );
        assert_eq!(prepared.groups[1].vertices[2].uv, [0.5, 0.5]);
        assert_eq!(prepared.bounds.center, [0.5, 0.5, 0.]);
        assert_eq!(prepared.groups[0].vertices[0].normal, [0., 0., 1.]);
        assert!(prepare(vec![], false).unwrap().is_none());
    }
}
