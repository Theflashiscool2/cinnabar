//! Native special-item submission, adapted from MIT item_render/special.rs at
//! Vasysik/horizon-android 5b8e6d0b6453e57fb23eb7bbb365bbc7edd5b9b2.
//! Uses Cinnabar's verified model-part data; no world placement or additional renderer owner.
use super::{Image, Quad, invalid};
use crate::entity::java::{Models, Part};
use crate::registry::java::{
    banner, skull,
    source::{Error, Identifier, Source},
};
use glam::{Mat4, Quat, Vec3};
use java_item_model::item_model::{ItemModelNode, ItemModels, SpecialKind, SpecialModel};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    f32::consts::PI,
    sync::Arc,
};
#[derive(Default)]
pub(super) struct Resources {
    pub models: Option<Arc<Models>>,
    images: BTreeMap<String, Arc<Image>>,
    banner_patterns: BTreeMap<String, String>,
    pottery_patterns: BTreeMap<String, String>,
}
fn directories(node: &ItemModelNode, out: &mut BTreeSet<String>) {
    match node {
        ItemModelNode::Composite { models, .. } => {
            for node in models {
                directories(node, out)
            }
        }
        ItemModelNode::Condition {
            on_true, on_false, ..
        } => {
            directories(on_true, out);
            directories(on_false, out)
        }
        ItemModelNode::Select {
            cases, fallback, ..
        } => {
            for case in cases {
                directories(&case.model, out)
            }
            if let Some(f) = fallback {
                directories(f, out)
            }
        }
        ItemModelNode::RangeDispatch {
            entries, fallback, ..
        } => {
            for (_, m) in entries {
                directories(m, out)
            }
            if let Some(f) = fallback {
                directories(f, out)
            }
        }
        ItemModelNode::Special { model, .. } => {
            let dir = match &model.kind {
                SpecialKind::Chest { .. } => "chest",
                SpecialKind::ShulkerBox { .. } => "shulker",
                SpecialKind::Banner { .. } => "banner",
                SpecialKind::Shield => "shield",
                SpecialKind::DecoratedPot => "decorated_pot",
                SpecialKind::Conduit => "conduit",
                SpecialKind::CopperGolemStatue { .. } => "copper_golem",
                SpecialKind::PlayerHead => "player",
                SpecialKind::Head { kind, .. } => match kind.as_str() {
                    "skeleton" | "wither_skeleton" => "skeleton",
                    "dragon" => "enderdragon",
                    "zombie" => "zombie",
                    "creeper" => "creeper",
                    "piglin" => "piglin",
                    _ => "player",
                },
                SpecialKind::Trident => "trident",
                SpecialKind::Bell => "bell",
                SpecialKind::Book { .. } => "enchantment",
                SpecialKind::EndCube { .. } => return,
            };
            out.insert(dir.into());
        }
        _ => {}
    }
}
impl Resources {
    pub fn load(source: &mut dyn Source, definitions: &ItemModels) -> Result<Self, Error> {
        let mut dirs = BTreeSet::new();
        for d in definitions.definitions.values() {
            directories(&d.model, &mut dirs)
        }
        let mut result = Self::default();
        let mut bytes = 0usize;
        for dir in &dirs {
            for path in source.entries(&format!("assets/minecraft/textures/entity/{dir}/"))? {
                if source.cancelled() {
                    return Err(invalid("cancelled special item resources"));
                }
                let Some(name) = path
                    .strip_prefix("assets/minecraft/textures/")
                    .and_then(|p| p.strip_suffix(".png"))
                else {
                    continue;
                };
                let image = Image::decode(&source.read(&path)?)?;
                bytes += image.rgba.len();
                if bytes > 16 * 1024 * 1024 {
                    return Err(Error::Budget("special item images (16 MiB)".into()));
                }
                result
                    .images
                    .insert(format!("minecraft:{name}"), Arc::new(image));
            }
        }
        if dirs.contains("banner") || dirs.contains("shield") {
            let mut images = BTreeMap::new();
            result.banner_patterns = banner::load(source, &mut images)?;
        }
        if dirs.contains("decorated_pot") {
            let mut images = BTreeMap::new();
            result.pottery_patterns = crate::registry::java::pot::load(source, &mut images)?;
            for (name, image) in images {
                result.images.insert(name.replace(":textures/", ":"), image);
            }
        }
        Ok(result)
    }
    fn image(&self, name: &str) -> Result<Arc<Image>, Error> {
        self.images
            .get(name)
            .cloned()
            .ok_or_else(|| invalid(&format!("special renderer texture unavailable: {name}")))
    }
    fn part(&self, name: &str) -> Result<&Part, Error> {
        self.models
            .as_ref()
            .and_then(|m| m.get(name))
            .ok_or_else(|| invalid(&format!("special renderer model unavailable: {name}")))
    }
    /// Raw native SkullModel, before item-model display or block mounting transforms.
    pub(super) fn player_head(
        &self,
        argument: &Value,
        heads: &super::HeadTextures,
    ) -> Result<Vec<Quad>, Error> {
        let mut out = Vec::new();
        if !argument.is_null() {
            let texture = heads
                .textures
                .get(&super::profile_key(argument))
                .ok_or_else(|| invalid("unresolved item head profile"))?;
            let image = match texture {
                super::HeadTexture::Image(image) => image.clone(),
                super::HeadTexture::Pack(path) => {
                    let id = Identifier::parse(path)?;
                    let path = id.path().strip_prefix("textures/").unwrap_or(id.path());
                    let path = path.strip_suffix(".png").unwrap_or(path);
                    self.image(&format!("{}:{path}", id.namespace()))?
                }
            };
            emit(
                self.part("item/player_head/main")?,
                Mat4::IDENTITY,
                &none,
                image,
                u32::MAX,
                &mut out,
            );
            return Ok(out);
        }
        let texture = format!(
            "minecraft:{}",
            skull::defaults::unprofiled()
                .strip_prefix("textures/")
                .ok_or_else(|| invalid("native default skin path"))?
        );
        emit(
            self.part("item/player_head/main")?,
            Mat4::IDENTITY,
            &none,
            self.image(&texture)?,
            u32::MAX,
            &mut out,
        );
        Ok(out)
    }
    pub fn geometry(
        &self,
        model: &SpecialModel,
        argument: &Value,
        heads: &super::HeadTextures,
    ) -> Result<Vec<Quad>, Error> {
        let mut out = Vec::new();
        let mut draw = |name: &str,
                        texture: &str,
                        color: u32,
                        adjust: &dyn Fn(&str, &mut [f32; 9])|
         -> Result<(), Error> {
            emit(
                self.part(name)?,
                Mat4::IDENTITY,
                adjust,
                self.image(texture)?,
                color,
                &mut out,
            );
            Ok(())
        };
        match &model.kind {
            SpecialKind::Chest {
                texture,
                openness,
                chest_type,
            } => {
                let name = match chest_type.as_str() {
                    "single" => "chest",
                    "left" => "double_chest_left",
                    "right" => "double_chest_right",
                    _ => return Err(invalid("unknown item chest half")),
                };
                draw(
                    &format!("item/{name}/main"),
                    &mapped("chest", texture)?,
                    u32::MAX,
                    &chest_pose(*openness),
                )?;
            }
            SpecialKind::ShulkerBox { texture, openness } => draw(
                "item/shulker_box/main",
                &mapped("shulker", texture)?,
                u32::MAX,
                &shulker_pose(*openness),
            )?,
            SpecialKind::Head {
                kind,
                texture,
                animation,
            } => {
                let (layer, skin) = head(kind)?;
                let texture = texture
                    .as_ref()
                    .map(|t| mapped("", t))
                    .transpose()?
                    .unwrap_or_else(|| skin.into());
                draw(layer, &texture, u32::MAX, &skull_pose(kind, *animation))?;
            }
            SpecialKind::PlayerHead => return self.player_head(argument, heads),
            SpecialKind::CopperGolemStatue { texture, pose } => {
                let name = match pose.as_str() {
                    "standing" => "statue_standing",
                    "running" => "statue_running",
                    "sitting" => "statue_sitting",
                    "star" => "statue_star",
                    _ => return Err(invalid("unknown item statue pose")),
                };
                let id = Identifier::parse(texture)?;
                let path = id
                    .path()
                    .strip_prefix("textures/")
                    .and_then(|p| p.strip_suffix(".png"))
                    .ok_or_else(|| invalid("statue item texture path"))?;
                draw(
                    name,
                    &format!("{}:{path}", id.namespace()),
                    u32::MAX,
                    &statue_pose,
                )?;
            }
            SpecialKind::Conduit => draw(
                "conduit_shell",
                "minecraft:entity/conduit/base",
                u32::MAX,
                &none,
            )?,
            SpecialKind::Banner { color, wall } => {
                let main = if *wall {
                    "wall_banner"
                } else {
                    "standing_banner"
                };
                draw(
                    &format!("item/{main}/main"),
                    "minecraft:entity/banner/banner_base",
                    u32::MAX,
                    &none,
                )?;
                draw(
                    &format!("item/{main}/flag"),
                    "minecraft:entity/banner/banner_base",
                    u32::MAX,
                    &flag_pose,
                )?;
                draw(
                    &format!("item/{main}/flag"),
                    "minecraft:entity/banner/base",
                    dye(&Value::String(color.clone()))?,
                    &flag_pose,
                )?;
                for (texture, color) in self.patterns(argument, "banner")? {
                    draw(&format!("item/{main}/flag"), &texture, color, &flag_pose)?
                }
            }
            SpecialKind::Shield => {
                let layers = self.patterns(&argument["minecraft:banner_patterns"], "shield")?;
                let base = &argument["minecraft:base_color"];
                let patterned = !layers.is_empty() || !base.is_null();
                let texture = if patterned {
                    "minecraft:entity/shield/shield_base"
                } else {
                    "minecraft:entity/shield/shield_base_nopattern"
                };
                draw("item/shield/main", texture, u32::MAX, &none)?;
                if patterned {
                    draw(
                        "item/shield/main",
                        "minecraft:entity/shield/base",
                        if base.is_null() { u32::MAX } else { dye(base)? },
                        &none,
                    )?;
                    for (texture, color) in layers {
                        draw("item/shield/main", &texture, color, &none)?
                    }
                }
            }
            SpecialKind::DecoratedPot => {
                let image = self.image("minecraft:entity/decorated_pot/decorated_pot_base")?;
                for name in ["neck", "top", "bottom"] {
                    let root = self.part("pot_base")?;
                    let p = child(root, name).ok_or_else(|| invalid("pot base part"))?;
                    emit(p, Mat4::IDENTITY, &none, image.clone(), u32::MAX, &mut out);
                }
                for (i, name) in ["back", "left", "right", "front"].into_iter().enumerate() {
                    let p = child(self.part("pot_sides")?, name)
                        .ok_or_else(|| invalid("pot side part"))?;
                    let value = if let Some(a) = argument
                        .get("sides")
                        .or(Some(argument))
                        .and_then(Value::as_array)
                    {
                        a.get(i)
                    } else {
                        argument.get(name)
                    };
                    let texture = self.pot_texture(value)?;
                    emit(
                        p,
                        Mat4::IDENTITY,
                        &none,
                        self.image(&texture)?,
                        u32::MAX,
                        &mut out,
                    );
                }
            }
            SpecialKind::Trident => draw(
                "item/trident/main",
                "minecraft:entity/trident/trident",
                u32::MAX,
                &none,
            )?,
            SpecialKind::Bell => draw(
                "item/bell/main",
                "minecraft:entity/bell/bell_body",
                u32::MAX,
                &none,
            )?,
            SpecialKind::Book {
                open_angle,
                page1,
                page2,
            } => draw(
                "item/book/main",
                "minecraft:entity/enchantment/enchanting_table_book",
                u32::MAX,
                &book_pose(*open_angle * PI / 180., *page1, *page2),
            )?,
            _ => {
                return Err(invalid(&format!(
                    "special item renderer {} is not integrated",
                    model.type_name()
                )));
            }
        }
        if out.is_empty() {
            return Err(invalid("special renderer produced no geometry"));
        }
        Ok(out)
    }
    fn pot_texture(&self, template: Option<&Value>) -> Result<String, Error> {
        let plain = "minecraft:entity/decorated_pot/decorated_pot_side";
        let Some(template) = template.filter(|v| !v.is_null()) else {
            return Ok(plain.into());
        };
        if template.get("count").and_then(Value::as_u64) == Some(0) {
            return Ok(plain.into());
        }
        let item = super::defaults::item_name(
            template
                .get("item")
                .or_else(|| template.get("id"))
                .unwrap_or(template),
        )
        .ok_or_else(|| invalid("pot item template identity"))?;
        let component = "minecraft:provides_pottery_pattern";
        if template
            .get("removed")
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(component)))
            || template
                .get("components")
                .and_then(|v| v.get("!minecraft:provides_pottery_pattern"))
                .is_some()
        {
            return Ok(plain.into());
        }
        let pattern = template
            .get("added")
            .or_else(|| template.get("components"))
            .and_then(|v| v.get(component))
            .or_else(|| super::defaults::component(item, component));
        let Some(pattern) = pattern else {
            return Ok(plain.into());
        };
        if let Some(asset) = pattern.get("asset_id").and_then(Value::as_str) {
            let id = Identifier::parse(asset)?;
            return Ok(format!(
                "{}:entity/decorated_pot/{}",
                id.namespace(),
                id.path()
            ));
        }
        let texture = pattern
            .as_str()
            .and_then(|id| self.pottery_patterns.get(id))
            .ok_or_else(|| invalid("pot template pattern holder"))?;
        Ok(texture.replace(":textures/", ":"))
    }
    fn patterns(&self, value: &Value, kind: &str) -> Result<Vec<(String, u32)>, Error> {
        if value.is_null() {
            return Ok(Vec::new());
        }
        let layers = value
            .as_array()
            .ok_or_else(|| invalid("item pattern layers"))?;
        layers
            .iter()
            .take(16)
            .map(|layer| {
                let pattern = &layer["pattern"];
                let asset = if let Some(name) = pattern.as_str() {
                    self.banner_patterns
                        .get(name)
                        .map(String::as_str)
                        .ok_or_else(|| invalid("unknown item banner pattern"))?
                } else {
                    pattern
                        .get("asset_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("inline banner asset"))?
                };
                let path = asset
                    .strip_prefix("textures/entity/banner/")
                    .or_else(|| asset.strip_prefix("minecraft:textures/entity/banner/"))
                    .unwrap_or(asset.strip_prefix("minecraft:").unwrap_or(asset));
                Ok((
                    format!("minecraft:entity/{kind}/{path}"),
                    dye(&layer["color"])?,
                ))
            })
            .collect()
    }
}
fn mapped(dir: &str, id: &str) -> Result<String, Error> {
    let id = Identifier::parse(id)?;
    Ok(if dir.is_empty() {
        format!("{}:entity/{}", id.namespace(), id.path())
    } else {
        format!("{}:entity/{dir}/{}", id.namespace(), id.path())
    })
}
fn dye(value: &Value) -> Result<u32, Error> {
    let rgb = if let Some(s) = value.as_str() {
        banner::DYES
            .iter()
            .find(|(name, _)| *name == s)
            .map(|(_, rgb)| *rgb)
    } else {
        value
            .as_u64()
            .and_then(|i| banner::DYES.get(i as usize))
            .map(|(_, rgb)| *rgb)
    }
    .ok_or_else(|| invalid("item dye color"))?;
    Ok(0xff000000 | rgb)
}
fn head(kind: &str) -> Result<(&'static str, &'static str), Error> {
    Ok(match kind {
        "skeleton" => (
            "item/skeleton_skull/main",
            "minecraft:entity/skeleton/skeleton",
        ),
        "wither_skeleton" => (
            "item/wither_skeleton_skull/main",
            "minecraft:entity/skeleton/wither_skeleton",
        ),
        "zombie" => ("item/zombie_head/main", "minecraft:entity/zombie/zombie"),
        "creeper" => ("item/creeper_head/main", "minecraft:entity/creeper/creeper"),
        "dragon" => (
            "item/dragon_skull/main",
            "minecraft:entity/enderdragon/dragon",
        ),
        "piglin" => ("item/piglin_head/main", "minecraft:entity/piglin/piglin"),
        "player" => (
            "item/player_head/main",
            "minecraft:entity/player/wide/steve",
        ),
        _ => return Err(invalid("unknown item head kind")),
    })
}
fn child<'a>(part: &'a Part, name: &str) -> Option<&'a Part> {
    part.children.iter().find(|p| p.name == name)
}
fn emit(
    part: &Part,
    parent: Mat4,
    adjust: &dyn Fn(&str, &mut [f32; 9]),
    image: Arc<Image>,
    color: u32,
    out: &mut Vec<Quad>,
) {
    if !part.visible {
        return;
    }
    let mut p: [f32; 9] = std::array::from_fn(|i| {
        if i < 3 {
            part.offset[i]
        } else if i < 6 {
            part.rotation[i - 3]
        } else {
            part.scale[i - 6]
        }
    });
    adjust(&part.name, &mut p);
    let pose = parent
        * Mat4::from_translation(Vec3::new(p[0], p[1], p[2]) / 16.)
        * Mat4::from_quat(Quat::from_euler(glam::EulerRot::ZYX, p[5], p[4], p[3]))
        * Mat4::from_scale(Vec3::new(p[6], p[7], p[8]));
    if !part.skip_draw {
        for face in &part.faces {
            let positions = face.vertices.map(|p| {
                pose.transform_point3(Vec3::new(p[0], p[1], p[2]) / 16.)
                    .to_array()
            });
            let normal = pose
                .inverse()
                .transpose()
                .transform_vector3(Vec3::from_array(face.normal))
                .normalize_or_zero()
                .to_array();
            out.push(Quad {
                positions,
                uv: face.vertices.map(|p| [p[3], p[4]]),
                image: image.clone(),
                color: [
                    ((color >> 16) & 255) as f32 / 255.,
                    ((color >> 8) & 255) as f32 / 255.,
                    (color & 255) as f32 / 255.,
                    (color >> 24) as f32 / 255.,
                ],
                block_light: true,
                cull_back: false,
                normal: Some(normal),
            });
        }
    }
    for child in &part.children {
        emit(child, pose, adjust, image.clone(), color, out)
    }
}
fn skull_pose(kind: &str, animation: f32) -> impl Fn(&str, &mut [f32; 9]) + '_ {
    move |name: &str, p: &mut [f32; 9]| match (kind, name) {
        (_, "head") => {
            p[3] = 0.0;
            p[4] = 0.0;
        }
        ("dragon", "jaw") => p[3] = (((animation * PI * 0.2) as f64).sin() as f32 + 1.0) * 0.2,
        ("piglin", "left_ear") => {
            p[5] = -((((animation * PI * 0.2 * 1.2) as f64).cos() + 2.5) as f32) * 0.2;
        }
        ("piglin", "right_ear") => {
            p[5] = ((((animation * PI * 0.2) as f64).cos() + 2.5) as f32) * 0.2;
        }
        _ => {}
    }
}
fn shulker_pose(progress: f32) -> impl Fn(&str, &mut [f32; 9]) {
    move |name: &str, p: &mut [f32; 9]| {
        if name == "lid" {
            p[0] = 0.0;
            p[1] = 24.0 - progress * 0.5 * 16.0;
            p[2] = 0.0;
            p[4] = 270.0 * progress * (PI / 180.0);
        }
    }
}
fn chest_pose(open: f32) -> impl Fn(&str, &mut [f32; 9]) {
    move |name: &str, p: &mut [f32; 9]| {
        if name == "lid" || name == "lock" {
            p[3] = -(open * (PI / 2.0));
        }
    }
}
/// `BannerFlagModel.setupAnim(0)`.
fn flag_pose(name: &str, p: &mut [f32; 9]) {
    if name == "flag" {
        p[3] = (-0.0125 + 0.01 * (2.0 * PI * 0.0f32).cos()) * PI;
    }
}
/// `BookModel.setupAnim`.
fn book_pose(openness: f32, flip1: f32, flip2: f32) -> impl Fn(&str, &mut [f32; 9]) {
    move |name: &str, p: &mut [f32; 9]| match name {
        "left_lid" => p[4] = PI + openness,
        "right_lid" => p[4] = -openness,
        "left_pages" => {
            p[4] = openness;
            p[0] = openness.sin();
        }
        "right_pages" => {
            p[4] = -openness;
            p[0] = openness.sin();
        }
        "flip_page1" => {
            p[4] = openness - openness * 2.0 * flip1;
            p[0] = openness.sin();
        }
        "flip_page2" => {
            p[4] = openness - openness * 2.0 * flip2;
            p[0] = openness.sin();
        }
        _ => {}
    }
}
/// `CopperGolemStatueModel.setupAnim`.
fn statue_pose(name: &str, p: &mut [f32; 9]) {
    if name == "root" {
        p[1] = 0.0;
        p[5] = PI;
    }
}

fn none(_: &str, _: &mut [f32; 9]) {}
