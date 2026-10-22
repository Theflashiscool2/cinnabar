//! Worker-owned conversion of Java models into shared graphics buffers, with native state IDs.
use super::{
    catalog::BlockModels,
    geometry::Quad,
    models::Face,
    selectors::{Selection, Variant},
    source::{Error, Identifier},
    textures::{Image, Sprite},
};
use crate::{
    Animation, BlockOverlay, Material, ModelQuad, ModelTemplate, RuntimeAssets, TextureArray,
    TextureMip, TextureRef,
};
use std::{collections::BTreeMap, sync::Arc};

type VariantKey = (Identifier, [u16; 3], bool);
fn key(v: &Variant) -> VariantKey {
    (v.model.clone(), v.rotations, v.uv_lock)
}
/// Immutable catalog belonging to exactly one graphics-resource revision.
pub struct Graphics {
    pub runtime: Arc<RuntimeAssets>,
    states: BTreeMap<u32, State>,
    pub templates: BTreeMap<u32, Template>,
}
struct State {
    selection: Arc<Selection>,
    variants: BTreeMap<VariantKey, Vec<u32>>,
}
#[derive(Clone, Debug)]
pub struct Template {
    pub quads: Arc<[Quad]>,
    /// Conservative full opaque faces; partial shapes never erase neighbouring surfaces.
    pub occludes: u8,
}
impl Graphics {
    pub fn contains(&self, id: u32) -> bool {
        self.states.contains_key(&id)
    }
    pub fn select(&self, id: u32, position: [i32; 3]) -> Result<Vec<u32>, Error> {
        let state = self
            .states
            .get(&id)
            .ok_or_else(|| Error::Model(format!("Java state {id} has no compiled graphics")))?;
        Ok(state
            .selection
            .choose(position)
            .into_iter()
            .flat_map(|v| state.variants[&key(v)].iter().copied())
            .collect())
    }
}
/// Builds a complete resource revision on a worker. Missing/special block visuals fail explicitly.
/// `states` are native Java IDs; no shared sequential block-visual table is populated.
pub fn compile(
    models: &mut BlockModels,
    states: impl IntoIterator<Item = u32>,
    max_page_layers: u32,
) -> Result<Graphics, Error> {
    if max_page_layers == 0 || max_page_layers as usize > crate::MAX_TEXTURE_LAYERS {
        return Err(invalid("graphics texture-array limit"));
    }
    let mut builder = Builder {
        overlay: BlockOverlay {
            max_page_layers,
            ..Default::default()
        },
        images: Vec::new(),
        sprites: BTreeMap::new(),
        materials: BTreeMap::new(),
    };
    let mut admitted = BTreeMap::new();
    let mut templates = BTreeMap::new();
    for id in states {
        if admitted.contains_key(&id) {
            continue;
        }
        let name = models.state_name(id)?.to_owned();
        let selection = models.selection(id)?;
        let mut variants = BTreeMap::new();
        for variant in selection.parts().iter().flatten() {
            if variants.contains_key(&key(variant)) {
                continue;
            }
            let quads = models.geometry(variant)?;
            if quads.is_empty()
                && !matches!(
                    name.as_str(),
                    "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
                )
            {
                return Err(invalid(&format!(
                    "{name} needs a special renderer; empty geometry is not air"
                )));
            }
            let mut parts = Vec::new();
            for part in quads.chunks(crate::MAX_MODEL_TEMPLATE_QUADS) {
                let template = builder.overlay.model_templates.len() as u32;
                if template as usize >= crate::MAX_MODEL_TEMPLATES {
                    return Err(Error::Budget("Java model templates".into()));
                }
                let start = builder.overlay.model_quads.len() as u32;
                let mut occludes = 0;
                for quad in part {
                    let material = builder.material(models, quad, tint(&name, quad.tint_index)?)?;
                    let flags = face_id(quad.shade_override.unwrap_or(quad.facing))
                        | quad.cull_face.map_or(0, |f| face_id(f) << 4);
                    let positions = quad.positions.map(|p| {
                        p.map(|v| (v * f32::from(crate::MODEL_POSITION_SCALE)).round() as i16)
                    });
                    let mut uvs = [[0; 2]; 4];
                    // Signed UVs need a renderer representation extension, never saturating casts.
                    for (out, input) in uvs.iter_mut().zip(quad.uvs) {
                        for (out, v) in out.iter_mut().zip(input) {
                            let v = (v * f32::from(crate::MODEL_UV_SCALE)).round();
                            if !(0. ..=u16::MAX as f32).contains(&v) {
                                return Err(invalid("signed or oversized Java model UV"));
                            }
                            *out = v as u16;
                        }
                    }
                    if builder.overlay.materials[material as usize].flags
                        & (crate::MATERIAL_FLAG_ALPHA_BLEND | crate::MATERIAL_FLAG_ALPHA_CUTOUT)
                        == 0
                        && full_face(quad)
                    {
                        occludes |= face_bit(quad.facing);
                    }
                    builder.overlay.model_quads.push(ModelQuad {
                        positions,
                        uvs,
                        material,
                        flags,
                    });
                }
                builder.overlay.model_templates.push(ModelTemplate {
                    quad_start: start,
                    quad_count: part.len() as u32,
                    flags: 0,
                });
                templates.insert(
                    template,
                    Template {
                        quads: Arc::from(part),
                        occludes,
                    },
                );
                parts.push(template);
            }
            variants.insert(key(variant), parts);
        }
        admitted.insert(
            id,
            State {
                selection,
                variants,
            },
        );
    }
    builder.finish_textures()?;
    // Only shared GPU resources are appended. Java state identity remains in `states` above.
    let base = RuntimeAssets::diagnostic();
    let runtime = Arc::new(base.with_block_overlay(base.visual_count() as u32, &builder.overlay)?);
    Ok(Graphics {
        runtime,
        states: admitted,
        templates,
    })
}
struct SpriteRef {
    texture: TextureRef,
    animation: u32,
    alpha: u32,
}
struct Builder {
    overlay: BlockOverlay,
    images: Vec<Image>,
    sprites: BTreeMap<Identifier, SpriteRef>,
    materials: BTreeMap<(Identifier, u32), u32>,
}
impl Builder {
    fn reference(&self, index: usize) -> Result<TextureRef, Error> {
        let limit = self.overlay.max_page_layers as usize;
        Ok(TextureRef::new(
            (index / limit + 1) as u32,
            (index % limit) as u32,
        )?)
    }
    fn sprite(&mut self, models: &mut BlockModels, id: &Identifier) -> Result<(), Error> {
        if self.sprites.contains_key(id) {
            return Ok(());
        }
        let sprite = Sprite::load(models.source(), id)?;
        let mut alpha = 0;
        for image in &sprite.frames {
            for pixel in image.rgba.as_chunks::<4>().0 {
                if pixel[3] == 0 {
                    alpha |= crate::MATERIAL_FLAG_ALPHA_CUTOUT;
                } else if pixel[3] != 255 {
                    alpha |= crate::MATERIAL_FLAG_ALPHA_BLEND;
                }
            }
        }
        if alpha & crate::MATERIAL_FLAG_ALPHA_BLEND != 0 {
            alpha = crate::MATERIAL_FLAG_ALPHA_BLEND;
        }
        let first = self.images.len();
        let texture = self.reference(first)?;
        for index in 0..sprite.frames.len() {
            self.reference(first + index)?;
        }
        self.images.extend(sprite.frames);
        let animation = if sprite.timeline.len() > 1 {
            let index = self.overlay.animations.len() as u32;
            let frame_start = self.overlay.animation_frames.len() as u32;
            for &frame in &sprite.timeline {
                self.overlay
                    .animation_frames
                    .push(self.reference(first + frame)?);
            }
            self.overlay.animations.push(Animation {
                frame_start,
                frame_count: self.overlay.animation_frames.len() as u32 - frame_start,
                ticks_per_frame: sprite.ticks,
                atlas_index: 0,
                atlas_tile_variant: 0,
                replicate: 1,
                flags: if sprite.interpolate {
                    crate::ANIMATION_FLAG_BLEND
                } else {
                    0
                },
            });
            index
        } else {
            crate::NO_ANIMATION
        };
        let texture = if animation == crate::NO_ANIMATION {
            self.reference(first + sprite.timeline.first().copied().unwrap_or(0))?
        } else {
            texture
        };
        self.sprites.insert(
            id.clone(),
            SpriteRef {
                texture,
                animation,
                alpha,
            },
        );
        Ok(())
    }
    fn material(&mut self, models: &mut BlockModels, quad: &Quad, tint: u32) -> Result<u32, Error> {
        let id = &quad.material.sprite;
        self.sprite(models, id)?;
        let sprite = &self.sprites[id];
        let mut flags = tint | sprite.alpha;
        if quad.material.force_translucent {
            flags = (flags & !crate::MATERIAL_FLAG_ALPHA_CUTOUT) | crate::MATERIAL_FLAG_ALPHA_BLEND;
        }
        if !quad.ambient_occlusion {
            flags |= crate::MATERIAL_FLAG_DISABLE_AO;
        }
        let key = (id.clone(), flags);
        if let Some(id) = self.materials.get(&key) {
            return Ok(*id);
        }
        let index = self.overlay.materials.len() as u32;
        self.overlay.materials.push(Material {
            texture: sprite.texture,
            flags,
            animation: sprite.animation,
            ..Material::unvaried()
        });
        self.materials.insert(key, index);
        Ok(index)
    }
    fn finish_textures(&mut self) -> Result<(), Error> {
        for page in self.images.chunks(self.overlay.max_page_layers as usize) {
            let size = page.iter().map(|i| i.width).max().unwrap();
            let mut mips: Vec<TextureMip> = Vec::new();
            for image in page {
                self.overlay
                    .texture_source_sizes
                    .push([image.width as u16, image.height as u16]);
                self.overlay.texture_source_grids.push(0);
                let chain = crate::build_texture_mip_chain(
                    image.square_layer(size)?.into_boxed_slice(),
                    size,
                )?;
                if mips.is_empty() {
                    mips = chain.into_vec();
                } else {
                    for (dest, src) in mips.iter_mut().zip(chain) {
                        let mut bytes = dest.rgba8.to_vec();
                        bytes.extend_from_slice(&src.rgba8);
                        dest.rgba8 = bytes.into_boxed_slice();
                    }
                }
            }
            self.overlay.textures.push(TextureArray {
                layers: page.len() as u32,
                mips: mips.into_boxed_slice(),
            });
        }
        Ok(())
    }
}
/// Java 26.3 BlockColors.register mappings for the initial terrain slice. Other callbacks are
/// explicit unsupported behavior; they must not inherit Bedrock colours or guessed white tint.
fn tint(name: &str, index: i32) -> Result<u32, Error> {
    if index < 0 {
        return Ok(0);
    }
    match (name, index) {
        (
            "minecraft:grass_block"
            | "minecraft:short_grass"
            | "minecraft:fern"
            | "minecraft:potted_fern"
            | "minecraft:bush",
            0,
        ) => Ok(crate::MATERIAL_FLAG_GRASS_TINT),
        _ => Err(invalid(&format!(
            "Java block tint callback for {name} index {index} is not implemented"
        ))),
    }
}
pub const fn face_id(face: Face) -> u32 {
    match face {
        Face::Down => 1,
        Face::Up => 2,
        Face::West => 3,
        Face::East => 4,
        Face::North => 5,
        Face::South => 6,
    }
}
pub const fn face_bit(face: Face) -> u8 {
    1 << (face_id(face) - 1)
}
fn full_face(q: &Quad) -> bool {
    let axis = match q.facing {
        Face::Down | Face::Up => 1,
        Face::West | Face::East => 0,
        _ => 2,
    };
    let value = if matches!(q.facing, Face::Up | Face::East | Face::South) {
        1.
    } else {
        0.
    };
    let mut corners = 0u8;
    for p in q.positions {
        if (p[axis] - value).abs() > 0.00001 {
            return false;
        }
        let mut corner = 0;
        let mut bit = 0;
        for (i, v) in p.into_iter().enumerate() {
            if i != axis {
                if v.abs() < 0.00001 {
                } else if (v - 1.).abs() < 0.00001 {
                    corner |= 1 << bit;
                } else {
                    return false;
                }
                bit += 1;
            }
        }
        corners |= 1 << corner;
    }
    corners == 15
}
fn invalid(message: &str) -> Error {
    Error::Model(message.into())
}

#[cfg(test)]
mod tests;
