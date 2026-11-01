//! Worker-owned conversion of Java models into shared graphics buffers, with native state IDs.
pub mod audit;
pub mod tints;
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
#[derive(Clone, Copy, Debug)]
pub struct FluidMaterials {
    pub still: u32,
    pub flowing: u32,
    pub overlay: Option<u32>,
}
fn key(v: &Variant) -> VariantKey {
    (v.model.clone(), v.rotations, v.uv_lock)
}
/// Immutable catalog belonging to exactly one graphics-resource revision.
pub struct Graphics {
    pub runtime: Arc<RuntimeAssets>,
    states: BTreeMap<u32, State>,
    pub templates: BTreeMap<u32, Template>,
    builder: Builder,
    pub fluid_facts: Option<Arc<super::fluids::Table>>,
    pub block_types: Arc<[u32]>,
    pub chests: BTreeMap<u32, super::special::Chest>,
    pub shulkers: BTreeMap<u32, super::special::Shulker>,
    pub portals: std::collections::BTreeSet<u32>,
    pub gateways: std::collections::BTreeSet<u32>,
    pub portal: Option<Arc<super::portal::Resources>>,
    pub conduits: std::collections::BTreeSet<u32>,
    pub conduit: Option<Arc<super::conduit::Resources>>,
    pub pots: BTreeMap<u32, super::pot::Pot>,
    pub pot_patterns: BTreeMap<String, String>,
    pub statues: BTreeMap<u32, super::statue::Statue>,
    pub special_models: BTreeMap<String, Arc<crate::entity::java::Part>>,
    pub skulls: BTreeMap<u32, super::skull::Skull>,
    pub banners: BTreeMap<u32, super::banner::Banner>,
    pub banner_patterns: BTreeMap<String, String>,
    pub special_atlas: Option<Arc<crate::RuntimeBlockEntityAssets>>,
}
/// The renderer that owns an admitted state's visible output. Unsupported special renderers
/// never become an admitted invisible state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presentation {
    Model,
    Invisible,
    Fluid,
    WaterloggedModel,
    Chest,
    Shulker,
    Banner,
    Skull,
    Statue,
    Pot,
    Conduit,
    Portal,
}
#[derive(Clone)]
struct State {
    presentation: Presentation,
    selection: Option<Arc<Selection>>,
    variants: BTreeMap<VariantKey, Vec<u32>>,
}
#[derive(Clone, Debug)]
pub struct Template {
    pub quads: Arc<[Quad]>,
    /// Conservative full opaque faces; partial shapes never erase neighbouring surfaces.
    pub occludes: u8,
}
impl Graphics {
    pub fn presentation(&self, id: u32) -> Option<Presentation> {
        self.states.get(&id).map(|s| s.presentation)
    }
    pub fn fluid_materials(&self, kind: super::fluids::Kind) -> Option<FluidMaterials> {
        self.builder.fluids.get(&kind).copied()
    }
    pub fn contains(&self, id: u32) -> bool {
        self.states.contains_key(&id)
    }
    pub fn select(&self, id: u32, position: [i32; 3]) -> Result<Vec<u32>, Error> {
        let state = self
            .states
            .get(&id)
            .ok_or_else(|| Error::Model(format!("Java state {id} has no compiled graphics")))?;
        let Some(selection) = &state.selection else {
            return Ok(Vec::new());
        };
        Ok(selection
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
        fluids: BTreeMap::new(),
        chests: BTreeMap::new(),
        shulkers: BTreeMap::new(),
        banners: BTreeMap::new(),
        skulls: BTreeMap::new(),
        statues: BTreeMap::new(),
        portals: Default::default(),
        gateways: Default::default(),
        portal: None,
        conduits: Default::default(),
        conduit: None,
        pots: BTreeMap::new(),
        pot_patterns: BTreeMap::new(),
        special_models: BTreeMap::new(),
        banner_patterns: BTreeMap::new(),
        special_images: BTreeMap::new(),
    };
    let mut admitted = BTreeMap::new();
    let mut templates = BTreeMap::new();
    append(models, states, &mut builder, &mut admitted, &mut templates)?;
    finish(
        builder,
        admitted,
        templates,
        models.fluids.clone(),
        models.block_types(),
    )
}
/// An append-only catalog revision, valid only against its exact predecessor.
/// Construction preserves every old material, template, animation and texture address.
pub struct Extension {
    previous: Arc<Graphics>,
    next: Arc<Graphics>,
    resources: crate::DrawResourceExtension,
}
impl Extension {
    pub fn resources(&self) -> &crate::DrawResourceExtension {
        &self.resources
    }
    pub fn graphics(&self) -> &Arc<Graphics> {
        &self.next
    }
    pub fn extends(&self, graphics: &Arc<Graphics>) -> bool {
        Arc::ptr_eq(&self.previous, graphics)
    }
}
/// Compiles only new state models on a worker. Existing meshes keep their resource addresses.
/// Source pixels are retained to rebuild compatible texture pages without rereading old assets.
pub fn extend(
    previous: Arc<Graphics>,
    models: &mut BlockModels,
    states: impl IntoIterator<Item = u32>,
) -> Result<Extension, Error> {
    let mut builder = previous.builder.clone();
    let mut admitted = previous.states.clone();
    let mut templates = previous.templates.clone();
    append(models, states, &mut builder, &mut admitted, &mut templates)?;
    let next = Arc::new(finish(
        builder,
        admitted,
        templates,
        previous.fluid_facts.clone(),
        Arc::clone(&previous.block_types),
    )?);
    let resources = crate::DrawResourceExtension::checked(
        Arc::clone(&previous.runtime),
        Arc::clone(&next.runtime),
    )
    .ok_or_else(|| invalid("Java graphics extension changed resident draw addresses"))?;
    Ok(Extension {
        previous,
        next,
        resources,
    })
}
fn append(
    models: &mut BlockModels,
    states: impl IntoIterator<Item = u32>,
    builder: &mut Builder,
    admitted: &mut BTreeMap<u32, State>,
    templates: &mut BTreeMap<u32, Template>,
) -> Result<(), Error> {
    let mut failures = audit::Report::default();
    for id in states {
        if admitted.contains_key(&id) {
            continue;
        }
        if models.cancelled() {
            return Err(invalid("Java asset load cancelled"));
        }
        failures.checked_states += 1;
        let result = (|| -> Result<(), Error> {
            let state = models.state(id)?.clone();
            let name = &state.name;
            if let Some(fluid) = models
                .fluids
                .as_ref()
                .and_then(|table| table.state(id))
                .cloned()
                && fluid.kind != super::fluids::Kind::Empty
            {
                builder.fluid(models, fluid.kind)?;
                if fluid.invisible {
                    admitted.insert(
                        id,
                        State {
                            presentation: Presentation::Fluid,
                            selection: None,
                            variants: BTreeMap::new(),
                        },
                    );
                    return Ok(());
                }
            }
            if let Some(chest) = super::special::Chest::from_state(
                &state,
                models
                    .block_id(id)
                    .ok_or_else(|| invalid("missing Java block registry identity"))?,
            )? {
                for half in [super::special::ChestHalf::Single, chest.half] {
                    let path = chest.texture_path(half);
                    if !builder.special_images.contains_key(&path) {
                        let bytes = models
                            .source()
                            .read(&format!("assets/minecraft/{path}.png"))?;
                        builder
                            .special_images
                            .insert(path, Arc::new(Image::decode(&bytes)?));
                    }
                }
                builder.chests.insert(id, chest);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Chest,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if let Some(shulker) = super::special::Shulker::from_state(
                models.state(id)?,
                models
                    .block_id(id)
                    .ok_or_else(|| invalid("Java block type missing"))?,
            )? {
                let name = shulker.texture.clone();
                if !builder.special_images.contains_key(&name) {
                    let bytes = models
                        .source()
                        .read(&format!("assets/minecraft/{name}.png"))?;
                    builder
                        .special_images
                        .insert(name, Arc::new(Image::decode(&bytes)?));
                }
                builder.shulkers.insert(id, shulker);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Shulker,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if let Some(banner) = super::banner::Banner::from_state(&state)? {
                if builder.banner_patterns.is_empty() {
                    builder.banner_patterns =
                        super::banner::load(models.source(), &mut builder.special_images)?;
                }
                builder.banners.insert(id, banner);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Banner,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if super::portal::matches(&state) || super::gateway::matches(&state) {
                if builder.portal.is_none() {
                    builder.portal = Some(Arc::new(super::portal::load(
                        models.source(),
                        &mut builder.special_images,
                    )?));
                }
                if super::gateway::matches(&state) {
                    if models.fluids.is_none() {
                        return Err(invalid("Java gateway requires native face-occlusion facts"));
                    }
                    super::gateway::load(models.source(), &mut builder.special_images)?;
                    builder.gateways.insert(id);
                } else {
                    builder.portals.insert(id);
                }
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Portal,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if super::conduit::matches(&state)? {
                if builder.conduit.is_none() {
                    builder.conduit = Some(Arc::new(super::conduit::load(
                        models,
                        &mut builder.special_images,
                    )?));
                }
                builder.conduits.insert(id);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Conduit,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if let Some(pot) = super::pot::Pot::from_state(
                &state,
                models
                    .block_id(id)
                    .ok_or_else(|| invalid("missing Java block registry identity"))?,
            )? {
                for (name, children) in [
                    ("pot_base", super::pot::BASE_PARTS),
                    ("pot_sides", super::pot::SIDE_PARTS),
                ] {
                    if !builder.special_models.contains_key(name) {
                        let root = models
                            .entity_models
                            .as_ref()
                            .and_then(|m| m.get(name))
                            .ok_or_else(|| invalid("Java pot hierarchy unavailable"))?;
                        if children
                            .iter()
                            .any(|n| !root.children.iter().any(|p| p.name == *n))
                        {
                            return Err(invalid("Java pot hierarchy incomplete"));
                        }
                        builder
                            .special_models
                            .insert(name.into(), Arc::new(root.clone()));
                    }
                }
                if builder.pot_patterns.is_empty() {
                    builder.pot_patterns =
                        super::pot::load(models.source(), &mut builder.special_images)?;
                }
                builder.pots.insert(id, pot);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Pot,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if let Some(statue) = super::statue::Statue::from_state(&state)? {
                if !builder.special_models.contains_key(statue.model) {
                    let root = models
                        .entity_models
                        .as_ref()
                        .and_then(|m| m.get(statue.model))
                        .ok_or_else(|| {
                            Error::Model(format!(
                                "native Java statue {} requires the pinned models.json report",
                                statue.model
                            ))
                        })?;
                    builder
                        .special_models
                        .insert(statue.model.into(), Arc::new(root.clone()));
                }
                if !builder.special_images.contains_key(statue.texture) {
                    let bytes = models
                        .source()
                        .read(&format!("assets/minecraft/{}.png", statue.texture))?;
                    builder
                        .special_images
                        .insert(statue.texture.into(), Arc::new(Image::decode(&bytes)?));
                }
                builder.statues.insert(id, statue);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Statue,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            if let Some(skull) = super::skull::Skull::from_state(&state)? {
                let textures: &[&str] = &[skull.texture];
                let defaults: Vec<_>;
                let textures = if skull.shape == super::skull::Shape::Player {
                    defaults = super::skull::defaults::all()
                        .iter()
                        .map(String::as_str)
                        .collect();
                    defaults.as_slice()
                } else {
                    textures
                };
                for &texture in textures {
                    if !builder.special_images.contains_key(texture) {
                        let bytes = models
                            .source()
                            .read(&format!("assets/minecraft/{texture}.png"))?;
                        let image = if skull.shape == super::skull::Shape::Player {
                            super::skull::skin::decode(&bytes)?
                        } else {
                            Image::decode(&bytes)?
                        };
                        builder
                            .special_images
                            .insert(texture.into(), Arc::new(image));
                    }
                }
                builder.skulls.insert(id, skull);
                admitted.insert(
                    id,
                    State {
                        presentation: Presentation::Skull,
                        selection: None,
                        variants: BTreeMap::new(),
                    },
                );
                return Ok(());
            }
            let selection = models.selection(id)?;
            let mut variants = BTreeMap::new();
            for variant in selection.parts().iter().flatten() {
                if variants.contains_key(&key(variant)) {
                    continue;
                }
                let quads = models.geometry(variant)?;
                if quads.is_empty() && !tints::allows_empty_model(&state) {
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
                        let material = builder.material(
                            models,
                            quad,
                            tints::material(&state, quad.tint_index)?,
                        )?;
                        let flags = tints::quad_flags(&state, quad.tint_index)?
                            | face_id(quad.shade_override.unwrap_or(quad.facing))
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
                    presentation: if models
                        .fluids
                        .as_ref()
                        .and_then(|t| t.state(id))
                        .is_some_and(|s| s.kind != super::fluids::Kind::Empty)
                    {
                        Presentation::WaterloggedModel
                    } else if variants.values().all(Vec::is_empty) {
                        Presentation::Invisible
                    } else {
                        Presentation::Model
                    },
                    selection: Some(selection),
                    variants,
                },
            );
            Ok(())
        })();
        if let Err(error) = result {
            failures.record(
                id,
                models.state_name(id).unwrap_or("unknown"),
                &error.to_string(),
            );
        }
    }
    if failures.failed_states == 0 {
        Ok(())
    } else {
        Err(Error::ModelBatch(failures))
    }
}
fn finish(
    mut builder: Builder,
    states: BTreeMap<u32, State>,
    templates: BTreeMap<u32, Template>,
    fluid_facts: Option<Arc<super::fluids::Table>>,
    block_types: Arc<[u32]>,
) -> Result<Graphics, Error> {
    builder.finish_textures()?;
    let special_atlas = super::special::atlas(&builder.special_images)?;
    let chests = builder.chests.clone();
    let shulkers = builder.shulkers.clone();
    let banners = builder.banners.clone();
    let skulls = builder.skulls.clone();
    let statues = builder.statues.clone();
    let portals = builder.portals.clone();
    let gateways = builder.gateways.clone();
    let portal = builder.portal.clone();
    let conduits = builder.conduits.clone();
    let conduit = builder.conduit.clone();
    let pots = builder.pots.clone();
    let pot_patterns = builder.pot_patterns.clone();
    let special_models = builder.special_models.clone();
    let banner_patterns = builder.banner_patterns.clone();
    let base = RuntimeAssets::diagnostic();
    let runtime = Arc::new(base.with_block_overlay(base.visual_count() as u32, &builder.overlay)?);
    // Keep source pixels and stable addresses, not a duplicate copy of the generated mip chains.
    builder.overlay.textures.clear();
    builder.overlay.texture_source_sizes.clear();
    builder.overlay.texture_source_grids.clear();
    Ok(Graphics {
        runtime,
        states,
        templates,
        builder,
        fluid_facts,
        chests,
        shulkers,
        banners,
        skulls,
        statues,
        portals,
        gateways,
        portal,
        conduits,
        conduit,
        pots,
        pot_patterns,
        special_models,
        banner_patterns,
        special_atlas,
        block_types,
    })
}
#[derive(Clone)]
struct SpriteRef {
    texture: TextureRef,
    animation: u32,
    alpha: u32,
}
#[derive(Clone)]
struct Builder {
    overlay: BlockOverlay,
    images: Vec<Arc<Image>>,
    sprites: BTreeMap<Identifier, SpriteRef>,
    materials: BTreeMap<(Identifier, u32), u32>,
    fluids: BTreeMap<super::fluids::Kind, FluidMaterials>,
    chests: BTreeMap<u32, super::special::Chest>,
    shulkers: BTreeMap<u32, super::special::Shulker>,
    banners: BTreeMap<u32, super::banner::Banner>,
    skulls: BTreeMap<u32, super::skull::Skull>,
    statues: BTreeMap<u32, super::statue::Statue>,
    portals: std::collections::BTreeSet<u32>,
    gateways: std::collections::BTreeSet<u32>,
    portal: Option<Arc<super::portal::Resources>>,
    conduits: std::collections::BTreeSet<u32>,
    conduit: Option<Arc<super::conduit::Resources>>,
    pots: BTreeMap<u32, super::pot::Pot>,
    pot_patterns: BTreeMap<String, String>,
    special_models: BTreeMap<String, Arc<crate::entity::java::Part>>,
    banner_patterns: BTreeMap<String, String>,
    special_images: BTreeMap<String, Arc<Image>>,
}
impl Builder {
    fn fluid(&mut self, models: &mut BlockModels, kind: super::fluids::Kind) -> Result<(), Error> {
        use super::fluids::Kind;
        if self.fluids.contains_key(&kind) {
            return Ok(());
        }
        let (name, flags) = match kind {
            Kind::Water => (
                "water",
                crate::MATERIAL_FLAG_WATER_TINT | crate::MATERIAL_FLAG_ALPHA_BLEND,
            ),
            Kind::Lava => ("lava", crate::MATERIAL_FLAG_LIQUID_DEPTH_WRITE),
            Kind::Empty => return Err(invalid("empty fluid has no material")),
        };
        let mut load = |suffix| -> Result<u32, Error> {
            let id = Identifier::parse(&format!("minecraft:block/{name}_{suffix}"))?;
            self.sprite(models, &id)?;
            let sprite = &self.sprites[&id];
            let index = self.overlay.materials.len() as u32;
            self.overlay.materials.push(Material {
                texture: sprite.texture,
                flags,
                animation: sprite.animation,
                ..Material::unvaried()
            });
            Ok(index)
        };
        let materials = FluidMaterials {
            still: load("still")?,
            flowing: load("flow")?,
            overlay: if kind == Kind::Water {
                Some(load("overlay")?)
            } else {
                None
            },
        };
        self.fluids.insert(kind, materials);
        Ok(())
    }
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
        self.images.extend(sprite.frames.into_iter().map(Arc::new));
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
