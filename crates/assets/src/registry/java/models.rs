//! Parent inheritance and texture-slot resolution for Java cuboid models.
use super::source::{Error, Identifier, Source};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    Down,
    Up,
    West,
    East,
    North,
    South,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FaceDefinition {
    pub texture: String,
    pub uv: Option<[f32; 4]>,
    pub cullface: Option<Face>,
    #[serde(default = "no_tint")]
    pub tintindex: i32,
    #[serde(default)]
    pub rotation: i32,
}
fn no_tint() -> i32 {
    -1
}
#[derive(Clone, Debug, Deserialize)]
pub struct Rotation {
    pub origin: [f32; 3],
    pub axis: Option<String>,
    pub angle: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    #[serde(default)]
    pub rescale: bool,
}
impl Rotation {
    pub fn angles(&self) -> Result<[f32; 3], Error> {
        let mut result = [0.; 3];
        if let Some(axis) = &self.axis {
            let index = match axis.as_str() {
                "x" => 0,
                "y" => 1,
                "z" => 2,
                _ => return Err(invalid("element rotation axis")),
            };
            result[index] = self
                .angle
                .ok_or_else(|| invalid("element rotation angle"))?;
        } else {
            if self.angle.is_some() || (self.x.is_none() && self.y.is_none() && self.z.is_none()) {
                return Err(invalid("element rotation value"));
            }
            result = [
                self.x.unwrap_or(0.),
                self.y.unwrap_or(0.),
                self.z.unwrap_or(0.),
            ];
        }
        if result.iter().chain(&self.origin).any(|v| !v.is_finite()) {
            return Err(invalid("nonfinite element rotation"));
        }
        Ok(result)
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub faces: BTreeMap<Face, FaceDefinition>,
    pub rotation: Option<Rotation>,
    pub shade_direction_override: Option<Face>,
    #[serde(default)]
    pub light_emission: u8,
}
#[derive(Clone, Debug)]
pub struct Material {
    pub sprite: Identifier,
    pub force_translucent: bool,
}
#[derive(Clone, Debug)]
pub struct Model {
    pub elements: Arc<[Element]>,
    pub textures: BTreeMap<String, Value>,
    pub ambient_occlusion: bool,
}
impl Model {
    pub fn material(&self, slot: &str) -> Result<Material, Error> {
        let mut seen = BTreeSet::new();
        let mut key = slot.strip_prefix('#').unwrap_or(slot);
        loop {
            if !seen.insert(key) {
                return Err(invalid("texture reference cycle"));
            }
            let value = self
                .textures
                .get(key)
                .ok_or_else(|| Error::Model(format!("missing texture slot {key}")))?;
            if let Some(reference) = value.as_str().and_then(|s| s.strip_prefix('#')) {
                key = reference;
                continue;
            }
            return material(value);
        }
    }
}
fn material(value: &Value) -> Result<Material, Error> {
    let (sprite, force_translucent) = if let Some(name) = value.as_str() {
        (name, false)
    } else {
        (
            value
                .get("sprite")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("texture material sprite"))?,
            value
                .get("force_translucent")
                .map(|v| {
                    v.as_bool()
                        .ok_or_else(|| invalid("texture material translucency"))
                })
                .transpose()?
                .unwrap_or(false),
        )
    };
    Ok(Material {
        sprite: Identifier::parse(sprite)?,
        force_translucent,
    })
}
#[derive(Default)]
pub struct Models {
    cache: BTreeMap<Identifier, Arc<Model>>,
}
impl Models {
    pub fn load(&mut self, source: &mut dyn Source, id: &Identifier) -> Result<Arc<Model>, Error> {
        self.resolve(source, id, &mut BTreeSet::new())
    }
    fn resolve(
        &mut self,
        source: &mut dyn Source,
        id: &Identifier,
        path: &mut BTreeSet<Identifier>,
    ) -> Result<Arc<Model>, Error> {
        if let Some(model) = self.cache.get(id) {
            return Ok(Arc::clone(model));
        }
        if path.len() > 64 || !path.insert(id.clone()) {
            return Err(Error::Model(format!("parent cycle/depth budget at {id}")));
        }
        let value = source.json(&id.asset_path("models", "json"))?;
        if !value.is_object() {
            return Err(invalid("model must be an object"));
        }
        let mut model = if let Some(parent) = value.get("parent") {
            let name = parent
                .as_str()
                .ok_or_else(|| invalid("model parent identifier"))?;
            if name.is_empty() {
                Model {
                    elements: Arc::from([]),
                    textures: BTreeMap::new(),
                    ambient_occlusion: true,
                }
            } else {
                (*self.resolve(source, &Identifier::parse(name)?, path)?).clone()
            }
        } else {
            Model {
                elements: Arc::from([]),
                textures: BTreeMap::new(),
                ambient_occlusion: true,
            }
        };
        if let Some(textures) = value.get("textures") {
            for (key, value) in textures
                .as_object()
                .ok_or_else(|| invalid("model textures object"))?
            {
                model.textures.insert(key.clone(), value.clone());
            }
        }
        if let Some(elements) = value.get("elements") {
            let elements: Vec<Element> = serde_json::from_value(elements.clone())?;
            for element in &elements {
                validate(element)?;
            }
            model.elements = Arc::from(elements);
        }
        if let Some(ao) = value.get("ambientocclusion") {
            model.ambient_occlusion = ao
                .as_bool()
                .ok_or_else(|| invalid("model ambient occlusion"))?;
        }
        path.remove(id);
        let model = Arc::new(model);
        self.cache.insert(id.clone(), Arc::clone(&model));
        Ok(model)
    }
}
fn validate(element: &Element) -> Result<(), Error> {
    if element
        .from
        .iter()
        .chain(&element.to)
        .any(|v| !v.is_finite() || !(-16. ..=32.).contains(v))
        || element.faces.is_empty()
        || element.light_emission > 15
    {
        return Err(invalid("element bounds/faces/emission"));
    }
    if let Some(rotation) = &element.rotation {
        rotation.angles()?;
    }
    for face in element.faces.values() {
        if face.rotation.rem_euclid(90) != 0
            || face.uv.is_some_and(|uv| uv.iter().any(|v| !v.is_finite()))
        {
            return Err(invalid("face UV/rotation"));
        }
    }
    Ok(())
}
fn invalid(message: &str) -> Error {
    Error::Model(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Memory(BTreeMap<String, String>);
    impl Source for Memory {
        fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
            self.0
                .get(path)
                .map(|s| s.as_bytes().to_vec())
                .ok_or_else(|| Error::Model(format!("missing {path}")))
        }
    }
    #[test]
    fn child_texture_overrides_resolve_in_inherited_faces() {
        let mut source=Memory(BTreeMap::from([
            ("assets/test/models/base.json".into(),r##"{"textures":{"side":"test:old","alias":"#side"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#alias"}}}]}"##.into()),
            ("assets/test/models/child.json".into(),r#"{"parent":"test:base","textures":{"side":{"sprite":"test:new","force_translucent":true}},"ambientocclusion":false}"#.into())]));
        let mut models = Models::default();
        let child = models
            .load(&mut source, &Identifier::parse("test:child").unwrap())
            .unwrap();
        let material = child
            .material(&child.elements[0].faces[&Face::Up].texture)
            .unwrap();
        assert_eq!(material.sprite.to_string(), "test:new");
        assert!(material.force_translucent);
        assert!(!child.ambient_occlusion);
        assert_eq!(
            models
                .load(&mut source, &Identifier::parse("test:base").unwrap())
                .unwrap()
                .material("#alias")
                .unwrap()
                .sprite
                .to_string(),
            "test:old"
        );
    }
    #[test]
    fn parent_and_texture_cycles_are_errors_not_empty_geometry() {
        let mut source = Memory(BTreeMap::from([(
            "assets/test/models/cycle.json".into(),
            r#"{"parent":"test:cycle"}"#.into(),
        )]));
        assert!(
            Models::default()
                .load(&mut source, &Identifier::parse("test:cycle").unwrap())
                .is_err()
        );
        let model = Model {
            elements: Arc::from([]),
            textures: BTreeMap::from([
                ("a".into(), Value::String("#b".into())),
                ("b".into(), Value::String("#a".into())),
            ]),
            ambient_occlusion: true,
        };
        assert!(model.material("#a").is_err());
        assert!(model.material("#missing").is_err());
    }
    #[test]
    fn explicit_empty_elements_replace_parent_geometry() {
        let mut source = Memory(BTreeMap::from([(
            "assets/test/models/empty.json".into(),
            r#"{"elements":[]}"#.into(),
        )]));
        assert!(
            Models::default()
                .load(&mut source, &Identifier::parse("test:empty").unwrap())
                .unwrap()
                .elements
                .is_empty()
        );
    }
}
