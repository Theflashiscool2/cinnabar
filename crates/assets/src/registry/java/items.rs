//! Native Java item definitions and cuboid/generated geometry, independent of inventory authority.
use super::{
    source::{Error, Identifier, Source},
    textures::{Image, Sprite},
};
use java_item_model::{
    item_bake::{self, SpriteLookup, SpriteMask},
    item_model::{self, ClientItem, Context, DisplayContext, ItemModels, LayerContent, StackView},
    model::CuboidModel,
    resolve::ModelLibrary,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub mod defaults;
mod raster;
mod special;
#[cfg(test)]
mod tests;

/// A stable artwork identity includes the entire authoritative patch and count-dependent selection.
pub fn key(
    item: &str,
    count: u32,
    added: &BTreeMap<String, Value>,
    removed: &BTreeSet<String>,
) -> String {
    let bytes = serde_json::to_vec(&(item, count, added, removed)).expect("JSON component values");
    format!("cinnabar-java:{:x}", Sha256::digest(bytes))
}

/// Render input copied from authoritative storage. It never mutates items or predicts transactions.
#[derive(Clone, Debug)]
pub struct Stack {
    pub item: String,
    pub count: u32,
    pub added: BTreeMap<String, Value>,
    pub removed: BTreeSet<String>,
}
impl Stack {
    pub fn key(&self) -> String {
        key(&self.item, self.count, &self.added, &self.removed)
    }
    fn view(&self) -> StackView {
        let mut patch: BTreeMap<_, _> = self
            .added
            .iter()
            .map(|(k, v)| (k.clone(), Some(v.clone())))
            .collect();
        for name in &self.removed {
            patch.insert(name.clone(), None);
        }
        StackView {
            item: self.item.clone(),
            count: i32::try_from(self.count).unwrap_or(i32::MAX),
            patch,
            bundle_selected: -1,
        }
    }
}

/// One fully selected, transformed native material quad. Positions are relative to the GUI origin.
#[derive(Clone)]
pub struct Quad {
    pub positions: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub image: Arc<Image>,
    pub color: [f32; 4],
    pub block_light: bool,
    pub cull_back: bool,
    pub normal: Option<[f32; 3]>,
}

/// Host-resolved public skins or admitted local texture patches, keyed by the opaque profile.
/// An absent download uses the target's UUID-selected default; it never rewrites a stack.
#[derive(Clone, Default)]
pub struct HeadTextures {
    pub revision: u64,
    pub textures: BTreeMap<String, HeadTexture>,
}
/// Immutable observations for remote held-item selection. Missing active-use durations are
/// explicit incompatibilities; callers never substitute an invented hand or charge clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerObservations {
    pub entity_type: Arc<str>,
    pub main_arm_left: bool,
    pub using_this_item: bool,
    pub use_item_remaining_ticks: Option<i32>,
    pub use_duration: Option<i32>,
}
/// The generic resolver's selected layers, tints and transforms in one display context.
/// Equality permits reusing identical prepared artwork without encoding model thresholds.
#[derive(Clone, Debug)]
pub struct Selection {
    origin: Arc<()>,
    display: View,
    selected: item_model::ResolvedItem,
}
impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.origin, &other.origin)
            && self.display == other.display
            && self.selected == other.selected
    }
}
/// Native item display context; GUI transforms are never reused as held geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Gui,
    Ground,
    FirstPersonRight,
    FirstPersonLeft,
    ThirdPersonRight,
    ThirdPersonLeft,
}
impl View {
    fn display(self) -> DisplayContext {
        match self {
            Self::Gui => DisplayContext::Gui,
            Self::Ground => DisplayContext::Ground,
            Self::FirstPersonRight => DisplayContext::FirstPersonRightHand,
            Self::FirstPersonLeft => DisplayContext::FirstPersonLeftHand,
            Self::ThirdPersonRight => DisplayContext::ThirdPersonRightHand,
            Self::ThirdPersonLeft => DisplayContext::ThirdPersonLeftHand,
        }
    }
}
#[derive(Clone)]
pub enum HeadTexture {
    Pack(String),
    Image(Arc<Image>),
}
pub fn profile_key(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON profile"))
    )
}
/// Native glint override and enchantments resolve through the same default-component owner.
pub fn glint(item: &str, added: &BTreeMap<String, Value>, removed: &BTreeSet<String>) -> bool {
    let component = |name: &str| {
        if removed.contains(name) {
            None
        } else {
            added.get(name).or_else(|| defaults::component(item, name))
        }
    };
    component("minecraft:enchantment_glint_override")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            component("minecraft:enchantments")
                .is_some_and(|v| v.as_object().is_some_and(|v| !v.is_empty()))
        })
}

/// Immutable resource generation. Shared by cancellable presentation workers, never a game tick owner.
pub struct Catalog {
    origin: Arc<()>,
    definitions: ItemModels,
    models: ModelLibrary,
    sprites: BTreeMap<String, Arc<Sprite>>,
    missing_sprites: BTreeMap<String, String>,
    missing_models: BTreeMap<String, String>,
    grass: Image,
    special: special::Resources,
}
impl SpriteLookup for Catalog {
    fn mask(&self, name: &str) -> Option<SpriteMask> {
        let sprite = self.sprites.get(name)?;
        let first = sprite.frames.first()?;
        Some(SpriteMask {
            width: first.width as usize,
            height: first.height as usize,
            frames: sprite
                .frames
                .iter()
                .map(|f| f.rgba.chunks_exact(4).map(|p| p[3] == 0).collect())
                .collect(),
        })
    }
}
impl item_model::ItemEnv for Catalog {
    fn potion_effect_color(&self, contents: &Value) -> Option<i32> {
        defaults::potion_color(contents)
    }
    fn grass_color(&self, temperature: f64, downfall: f64) -> i32 {
        let [r, g, b] = self
            .grass
            .climate_color(temperature as f32, downfall as f32)
            .expect("validated item grass map");
        (0xff000000u32 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)) as i32
    }
}
impl Catalog {
    /// Reads only resource paths, with Source budgets/cancellation. No archive is executed or extracted.
    pub fn load(source: &mut dyn Source, items: &[&str]) -> Result<Self, Error> {
        let mut definitions = ItemModels::default();
        for &item in items {
            if source.cancelled() {
                return Err(invalid("cancelled item resource load"));
            }
            let id = Identifier::parse(item)?;
            let bytes = source.read(&id.asset_path("items", "json"))?;
            let definition = ClientItem::from_json(
                std::str::from_utf8(&bytes).map_err(|_| invalid("item definition UTF-8"))?,
            )
            .map_err(model_error)?;
            definitions.definitions.insert(item.to_owned(), definition);
            definitions.prototypes.insert(
                item.to_owned(),
                defaults::components(item)
                    .ok_or_else(|| invalid("missing native item component prototype"))?
                    .clone(),
            );
        }
        let mut cuboids = BTreeMap::new();
        let mut pending = Vec::new();
        for definition in definitions.definitions.values() {
            definition.model.dependencies(&mut pending);
        }
        let mut visited = BTreeSet::new();
        let mut missing_models = BTreeMap::new();
        while let Some(name) = pending.pop() {
            if source.cancelled() {
                return Err(invalid("cancelled item model load"));
            }
            let id = Identifier::parse(&name)?;
            let name = format!("{}:{}", id.namespace(), id.path());
            if !visited.insert(name.clone()) || name == "minecraft:builtin/generated" {
                continue;
            }
            let model = (|| {
                let bytes = source.read(&id.asset_path("models", "json"))?;
                CuboidModel::from_json(
                    std::str::from_utf8(&bytes).map_err(|_| invalid("item model UTF-8"))?,
                )
                .map_err(model_error)
            })();
            match model {
                Ok(model) => {
                    pending.extend(model.referenced_models());
                    cuboids.insert(name, model);
                }
                Err(error) => {
                    missing_models.insert(name, error.to_string());
                }
            }
        }
        let names: Vec<_> = cuboids.keys().cloned().collect();
        let models = ModelLibrary::with_item_generator(cuboids).map_err(model_error)?;
        let mut materials = BTreeSet::new();
        for name in names {
            let Ok(resolved) = models.resolve(&name) else {
                continue;
            };
            for material in resolved.textures.values() {
                materials.insert(material.sprite.clone());
            }
        }
        let mut sprites = BTreeMap::new();
        let mut missing_sprites = BTreeMap::new();
        let mut bytes = 0usize;
        for name in materials {
            if source.cancelled() {
                return Err(invalid("cancelled item sprite load"));
            }
            match Sprite::load_entity(source, &Identifier::parse(&name)?) {
                Ok(sprite) => {
                    bytes = bytes
                        .checked_add(sprite.frames.iter().map(|f| f.rgba.len()).sum::<usize>())
                        .ok_or_else(|| invalid("item sprite size overflow"))?;
                    if bytes > 64 * 1024 * 1024 {
                        return Err(Error::Budget("item sprite generation (64 MiB)".into()));
                    }
                    sprites.insert(name, Arc::new(sprite));
                }
                Err(error) => {
                    missing_sprites.insert(name, error.to_string());
                }
            }
        }
        let grass = Image::decode(&source.read("assets/minecraft/textures/colormap/grass.png")?)?;
        grass.climate_color(0.5, 1.)?;
        let special = special::Resources::load(source, &definitions)?;
        Ok(Self {
            origin: Arc::new(()),
            definitions,
            models,
            sprites,
            missing_sprites,
            missing_models,
            grass,
            special,
        })
    }
    /// Model factories are supplied by the same hash-verified offline exporter used for world entities.
    pub fn with_entity_models(mut self, models: Arc<crate::entity::java::Models>) -> Self {
        self.special.models = Some(models);
        self.origin = Arc::new(());
        self
    }
    /// Resolves a GUI stack through its real item model tree; missing/special layers remain explicit.
    pub fn geometry(&self, stack: &Stack) -> Result<Vec<Quad>, Error> {
        self.geometry_with_heads(stack, &HeadTextures::default())
    }
    /// LivingEntityRenderer's player-skull branch bypasses item models and their display transforms.
    pub fn worn_player_head(
        &self,
        stack: &Stack,
        heads: &HeadTextures,
    ) -> Result<Option<Vec<Quad>>, Error> {
        if stack.item != "minecraft:player_head" {
            return Ok(None);
        }
        let profile =
            super::equipment::component(stack, "minecraft:profile").unwrap_or(&Value::Null);
        self.special.player_head(profile, heads).map(Some)
    }
    pub fn geometry_with_heads(
        &self,
        stack: &Stack,
        heads: &HeadTextures,
    ) -> Result<Vec<Quad>, Error> {
        self.geometry_in(stack, heads, View::Gui)
    }
    /// Applies the selected native display transform before crossing the shared mesh boundary.
    pub fn geometry_in(
        &self,
        stack: &Stack,
        heads: &HeadTextures,
        view: View,
    ) -> Result<Vec<Quad>, Error> {
        let selected = self.select_in(stack, view, None)?;
        self.geometry_selected(&selected, heads)
    }
    /// Selects resources using the existing native resolver; no texture decoding or baking occurs.
    pub fn select_in(
        &self,
        stack: &Stack,
        display: View,
        owner: Option<&OwnerObservations>,
    ) -> Result<Selection, Error> {
        let owner = owner
            .map(|owner| {
                let (duration, remaining) = if owner.using_this_item {
                    (
                        owner
                            .use_duration
                            .ok_or_else(|| invalid("native item use duration unavailable"))?,
                        owner.use_item_remaining_ticks.ok_or_else(|| {
                            invalid("native item remaining use ticks unavailable")
                        })?,
                    )
                } else {
                    (0, 0)
                };
                Ok::<_, Error>(item_model::OwnerState {
                    entity_type: owner.entity_type.to_string(),
                    main_arm_left: owner.main_arm_left,
                    using_this_item: owner.using_this_item,
                    use_duration: duration,
                    use_item_remaining_ticks: remaining,
                    ..Default::default()
                })
            })
            .transpose()?;
        let view = stack.view();
        let context = Context {
            display: display.display(),
            owner: owner.as_ref(),
            level: None,
            seed: 0,
        };
        let components = self.definitions.components(&view);
        if components
            .get("minecraft:potion_contents")
            .is_some_and(|v| !defaults::validate_potion(v))
        {
            return Err(invalid("unknown native potion/effect reference"));
        }
        Ok(Selection {
            origin: Arc::clone(&self.origin),
            display,
            selected: self.definitions.resolve(&view, &context, self),
        })
    }
    /// Bakes one immutable selection from this catalog through the shared material/geometry path.
    pub fn geometry_selected(
        &self,
        selection: &Selection,
        heads: &HeadTextures,
    ) -> Result<Vec<Quad>, Error> {
        if !Arc::ptr_eq(&self.origin, &selection.origin) {
            return Err(invalid(
                "native item selection belongs to another catalog generation",
            ));
        }
        let display = selection.display.display();
        let mut quads = Vec::new();
        for layer in &selection.selected.layers {
            let (name, special) = match &layer.content {
                LayerContent::Model { model } => (model.as_str(), None),
                LayerContent::Missing => {
                    return Err(invalid("item definition or model is missing"));
                }
                LayerContent::Special {
                    base,
                    model,
                    argument,
                } => (
                    base.as_str(),
                    Some(self.special.geometry(model, argument, heads)?),
                ),
            };
            if let Some(error) = self.missing_models.get(name) {
                return Err(Error::Model(format!("{name}: {error}")));
            }
            // The dependency's baker has a missing-model fallback; admission forbids using it here.
            self.models.resolve(name).map_err(model_error)?;
            let baked =
                item_bake::bake_item_model(&self.models, self, name).map_err(model_error)?;
            let matrix = baked.layer_matrix(display, &layer.transform);
            if let Some(special) = special {
                let normal_matrix =
                    glam::Mat3::from_mat4(glam::Mat4::from_cols_array_2d(&matrix.0))
                        .inverse()
                        .transpose();
                for mut q in special {
                    q.positions = q.positions.map(|p| transform(&matrix, p));
                    q.normal = q.normal.map(|n| {
                        (normal_matrix * glam::Vec3::from_array(n))
                            .normalize_or_zero()
                            .to_array()
                    });
                    q.block_light = baked.uses_block_light;
                    quads.push(q);
                }
                continue;
            }
            for q in &baked.quads {
                let sprite = self.sprites.get(&q.material.sprite).ok_or_else(|| {
                    Error::Model(format!(
                        "native item sprite {}: {}",
                        q.material.sprite,
                        self.missing_sprites
                            .get(&q.material.sprite)
                            .map(String::as_str)
                            .unwrap_or("not present")
                    ))
                })?;
                let image = Arc::new(sprite.frames[sprite.timeline[0]].clone());
                let positions = q.positions.map(|p| transform(&matrix, p));
                if positions.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(invalid("nonfinite item geometry"));
                }
                let tint = usize::try_from(q.tint_index)
                    .ok()
                    .and_then(|i| layer.tints.get(i))
                    .copied()
                    .unwrap_or(-1) as u32;
                quads.push(Quad {
                    positions,
                    uv: q.uv,
                    image,
                    color: [
                        ((tint >> 16) & 255) as f32 / 255.,
                        ((tint >> 8) & 255) as f32 / 255.,
                        (tint & 255) as f32 / 255.,
                        ((tint >> 24) & 255) as f32 / 255.,
                    ],
                    block_light: baked.uses_block_light,
                    cull_back: true,
                    normal: None,
                });
            }
        }
        Ok(quads)
    }
    pub fn icon(&self, stack: &Stack) -> Result<Image, Error> {
        Ok(raster::render(&self.geometry(stack)?))
    }
    pub fn icon_with_heads(&self, stack: &Stack, heads: &HeadTextures) -> Result<Image, Error> {
        Ok(raster::render(&self.geometry_with_heads(stack, heads)?))
    }
}
fn transform(matrix: &java_item_model::math::Mat4, p: [f32; 3]) -> [f32; 3] {
    let m = &matrix.0;
    std::array::from_fn(|i| m[0][i] * p[0] + m[1][i] * p[1] + m[2][i] * p[2] + m[3][i])
}
fn invalid(message: &str) -> Error {
    Error::Model(message.into())
}
fn model_error(error: java_item_model::DecodeError) -> Error {
    invalid(&error.to_string())
}
