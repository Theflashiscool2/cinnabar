//! Java pot hierarchy, named item-template decorations and server-event wobble.
use super::{atlas::BlockEntityAtlas, mesh::MeshBuilder};
use assets::{
    entity::java::Part,
    registry::java::{
        pot::{BASE, Pot, SIDE},
        source::Identifier,
    },
};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};
use world::NbtValue;

#[derive(Clone, Debug)]
pub struct Model {
    pub state: Pot,
    pub base: Arc<Part>,
    pub sides: Arc<Part>,
    /// Back, left, right, front; the wire uses names, not a legacy positional list.
    pub textures: [String; 4],
    pub wobble: Option<(u8, f32)>,
}
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state
            && Arc::ptr_eq(&self.base, &other.base)
            && Arc::ptr_eq(&self.sides, &other.sides)
            && self.textures == other.textures
            && self.wobble == other.wobble
    }
}
impl Model {
    /// Missing BE data is the native undecorated pot. Unsupported holders fail explicitly.
    pub fn from_data(
        state: Pot,
        data: Option<&NbtValue>,
        registry: &BTreeMap<String, String>,
        models: &BTreeMap<String, Arc<Part>>,
    ) -> Result<Self, String> {
        let mut textures = std::array::from_fn(|_| SIDE.to_owned());
        if let Some(data) = data {
            let NbtValue::Compound(data) = data else {
                return Err("Java pot compound expected".into());
            };
            if let Some(sherds) = data.get("sherds") {
                let NbtValue::Compound(sherds) = sherds else {
                    return Err("Java26.3 named pot decorations expected".into());
                };
                for (index, key) in ["back", "left", "right", "front"].into_iter().enumerate() {
                    if let Some(item) = sherds.get(key) {
                        textures[index] = item_texture(item, registry)?;
                    }
                }
            }
        }
        let part = |name| {
            models
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Java pot model missing: {name}"))
        };
        Ok(Self {
            state,
            base: part("pot_base")?,
            sides: part("pot_sides")?,
            textures,
            wobble: None,
        })
    }
}
fn item_texture(item: &NbtValue, registry: &BTreeMap<String, String>) -> Result<String, String> {
    let (id, patch) = match item {
        NbtValue::String(id) => (id.as_ref(), None),
        NbtValue::Compound(item) => (
            item.string("id").ok_or("Java pot item id missing")?,
            item.get("components"),
        ),
        _ => return Err("Java pot item template expected".into()),
    };
    let id = Identifier::parse(id).map_err(|e| e.to_string())?;
    if let Some(patch) = patch {
        let NbtValue::Compound(patch) = patch else {
            return Err("Java pot item component patch expected".into());
        };
        if patch.get("!minecraft:provides_pottery_pattern").is_some() {
            return Ok(SIDE.into());
        }
        if let Some(holder) = patch.get("minecraft:provides_pottery_pattern") {
            return match holder {
                NbtValue::String(key) => registry
                    .get(key.as_ref())
                    .cloned()
                    .ok_or_else(|| "Java pot pattern holder unavailable".into()),
                NbtValue::Compound(data) => {
                    let id = Identifier::parse(
                        data.string("asset_id")
                            .ok_or("Java pot pattern asset missing")?,
                    )
                    .map_err(|e| e.to_string())?;
                    let texture = format!(
                        "{}:textures/entity/decorated_pot/{}",
                        id.namespace(),
                        id.path()
                    );
                    registry
                        .values()
                        .any(|t| t == &texture)
                        .then_some(texture)
                        .ok_or_else(|| "Java pot inline pattern artwork unavailable".into())
                }
                _ => Err("Java pot pattern holder expected".into()),
            };
        }
    }
    // Checked against all pinned DataComponentInitializers item defaults by the reference fixture.
    let key = assets::registry::java::pot::default_pattern(&id);
    Ok(key
        .and_then(|k| registry.get(&k).cloned())
        .unwrap_or_else(|| SIDE.into()))
}
fn transform(block: [i32; 3], model: &Model) -> Mat4 {
    let center = Vec3::splat(0.5);
    let mut pose = Mat4::from_translation(Vec3::from_array(block.map(|n| n as f32)) + center)
        * Mat4::from_rotation_y(
            std::f32::consts::PI - f32::from(model.state.turn) * std::f32::consts::FRAC_PI_2,
        )
        * Mat4::from_translation(-center);
    if let Some((style, t)) = model.wobble.filter(|(_, t)| (0. ..=1.).contains(t)) {
        let pivot = Vec3::new(0.5, 0., 0.5);
        let rotation = if style == 0 {
            let phase = t * std::f32::consts::TAU;
            Mat4::from_rotation_x(-1.5 * (phase.cos() + 0.5) * (phase / 2.).sin() / 64.)
                * Mat4::from_rotation_z(phase.sin() / 64.)
        } else {
            Mat4::from_rotation_y((-t * 3. * std::f32::consts::PI).sin() * 0.125 * (1. - t))
        };
        pose = pose * Mat4::from_translation(pivot) * rotation * Mat4::from_translation(-pivot);
    }
    pose * Mat4::from_scale(Vec3::splat(1. / 16.))
}
pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
) {
    let pose = transform(block, model);
    if let Some(texture) = builder.required_texture(atlas, BASE, [32.; 2]) {
        super::java_parts::part(builder, &model.base, pose, texture, None);
    }
    for (name, texture) in ["back", "left", "right", "front"]
        .into_iter()
        .zip(&model.textures)
    {
        let Some(part) = model.sides.children.iter().find(|p| p.name == name) else {
            continue;
        };
        if let Some(texture) = builder.required_texture(atlas, texture, [16.; 2]) {
            super::java_parts::part(builder, part, pose, texture, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_templates_components_and_native_default_pattern_mapping() {
        let patterns = BTreeMap::from([
            ("minecraft:archer".into(), "archer".into()),
            ("minecraft:skull".into(), "skull".into()),
        ]);
        // NBT is constructed explicitly; server content is never interpreted as paths.
        let item = |id: &str, patch: Option<NbtValue>| {
            let mut fields = world::NbtCompound::default();
            fields.insert("id", NbtValue::String(id.into()));
            if let Some(p) = patch {
                fields.insert("components", p);
            }
            NbtValue::Compound(fields)
        };
        assert_eq!(
            item_texture(&item("minecraft:archer_pottery_sherd", None), &patterns).unwrap(),
            "archer"
        );
        assert_eq!(
            item_texture(&item("minecraft:brick", None), &patterns).unwrap(),
            SIDE
        );
        let mut patch = world::NbtCompound::default();
        patch.insert(
            "minecraft:provides_pottery_pattern",
            NbtValue::String("minecraft:skull".into()),
        );
        assert_eq!(
            item_texture(
                &item("minecraft:brick", Some(NbtValue::Compound(patch))),
                &patterns
            )
            .unwrap(),
            "skull"
        );
        let mut patch = world::NbtCompound::default();
        patch.insert(
            "!minecraft:provides_pottery_pattern",
            NbtValue::Compound(world::NbtCompound::default()),
        );
        assert_eq!(
            item_texture(
                &item(
                    "minecraft:archer_pottery_sherd",
                    Some(NbtValue::Compound(patch))
                ),
                &patterns
            )
            .unwrap(),
            SIDE
        );
        assert!(item_texture(&item("../private", None), &patterns).is_err());
        let root = Arc::new(Part {
            name: "root".into(),
            offset: [0.; 3],
            rotation: [0.; 3],
            scale: [1.; 3],
            visible: true,
            skip_draw: false,
            faces: vec![],
            children: vec![],
        });
        let models = BTreeMap::from([
            ("pot_base".into(), Arc::clone(&root)),
            ("pot_sides".into(), root),
        ]);
        let state = Pot {
            block_id: 1,
            turn: 0,
        };
        let mut sherds = world::NbtCompound::default();
        sherds.insert("front", item("minecraft:archer_pottery_sherd", None));
        sherds.insert("left", item("minecraft:skull_pottery_sherd", None));
        let mut data = world::NbtCompound::default();
        data.insert("sherds", NbtValue::Compound(sherds));
        let model = Model::from_data(
            state.clone(),
            Some(&NbtValue::Compound(data)),
            &patterns,
            &models,
        )
        .unwrap();
        assert_eq!(
            model.textures,
            [SIDE, "skull", SIDE, "archer"].map(str::to_owned)
        );
        assert_eq!(
            Model::from_data(state.clone(), None, &patterns, &models)
                .unwrap()
                .textures,
            [SIDE; 4].map(str::to_owned)
        );
        let mut invalid = world::NbtCompound::default();
        invalid.insert("sherds", NbtValue::List(vec![]));
        assert!(
            Model::from_data(
                state,
                Some(&NbtValue::Compound(invalid)),
                &patterns,
                &models
            )
            .is_err()
        );

        if let Ok(path) = std::env::var("CINNABAR_JAVA_POT_ITEMS") {
            let text = std::fs::read_to_string(path).unwrap();
            let mut count = 0;
            for line in text.lines().filter_map(|l| l.split("[STDOUT]: ").nth(1)) {
                let cols: Vec<_> = line.split_whitespace().collect();
                if cols.len() != 2 || !cols[0].starts_with("minecraft:") {
                    continue;
                }
                let registry = BTreeMap::from([(cols[1].into(), "pattern".into())]);
                assert_eq!(
                    item_texture(&item(cols[0], None), &registry).unwrap(),
                    "pattern"
                );
                count += 1;
            }
            assert_eq!(count, 23, "all native default pottery components");
        } else {
            eprintln!("missing fixture: CINNABAR_JAVA_POT_ITEMS");
        }
    }
    #[test]
    fn native_hierarchy_matches_all_twenty_eight_reference_parts_and_facings() {
        let (Ok(root), Ok(path)) = (
            std::env::var("CINNABAR_JAVA_TEST_ASSETS"),
            std::env::var("CINNABAR_JAVA_POT_VERTICES"),
        ) else {
            eprintln!("missing fixtures: CINNABAR_JAVA_TEST_ASSETS/CINNABAR_JAVA_POT_VERTICES");
            return;
        };
        let pin: serde_json::Value =
            serde_json::from_str(include_str!("../../../java-protocol/data/target.json")).unwrap();
        let pin = &pin["entity_models_report"];
        let models = assets::entity::java::Models::load(
            &std::fs::read(std::path::Path::new(&root).join("models.json")).unwrap(),
            pin["sha256"].as_str().unwrap(),
            pin["bytes"].as_u64().unwrap(),
        )
        .unwrap();
        let images = BTreeMap::from([(
            "test".into(),
            Arc::new(assets::registry::java::textures::Image {
                width: 32,
                height: 32,
                rgba: vec![255; 32 * 32 * 4],
            }),
        )]);
        let artwork = assets::registry::java::special::atlas(&images)
            .unwrap()
            .unwrap();
        let atlas = BlockEntityAtlas::from_assets(&artwork);
        let size = atlas.size();
        let text = std::fs::read_to_string(path).unwrap();
        let mut count = 0;
        for case in text.split("END\n").filter(|s| !s.trim().is_empty()) {
            let mut lines = case.lines();
            let header: Vec<_> = lines.next().unwrap().split_whitespace().collect();
            let turn = match header[0] {
                "south" => 0,
                "west" => 1,
                "north" => 2,
                "east" => 3,
                _ => panic!("invalid reference facing"),
            };
            let model = Model {
                state: Pot { block_id: 0, turn },
                base: Arc::new(models.get("pot_base").unwrap().clone()),
                sides: Arc::new(models.get("pot_sides").unwrap().clone()),
                textures: std::array::from_fn(|_| "test".into()),
                wobble: None,
            };
            let part = model
                .base
                .children
                .iter()
                .chain(&model.sides.children)
                .find(|p| p.name == header[1])
                .unwrap();
            let mut mesh = MeshBuilder::new(size);
            mesh.actor_light = 1;
            let tex = mesh.required_texture(&atlas, "test", [32.; 2]).unwrap();
            super::super::java_parts::part(&mut mesh, part, transform([0; 3], &model), tex, None);
            let expected: Vec<Vec<f32>> = lines
                .map(|l| l.split_whitespace().map(|n| n.parse().unwrap()).collect())
                .collect();
            // The native top/bottom factories also submit collapsed edge quads. Compare every
            // rasterizable reference face; assert our mesh contains no collapsed geometry.
            let expected: Vec<_> = expected
                .chunks_exact(4)
                .filter(|face| {
                    let point = |i: usize| Vec3::new(face[i][0], face[i][1], face[i][2]);
                    (point(1) - point(0))
                        .cross(point(2) - point(0))
                        .length_squared()
                        >= 1e-16
                })
                .flatten()
                .cloned()
                .collect();
            let actual: Vec<_> = mesh
                .solid
                .iter()
                .map(|v| {
                    [
                        v.position[0],
                        v.position[1],
                        v.position[2],
                        v.uv[0] * size[0] as f32 / 32.,
                        v.uv[1] * size[1] as f32 / 32.,
                        v.normal[0],
                        v.normal[1],
                        v.normal[2],
                    ]
                })
                .collect();
            let close =
                |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.00001);
            assert_eq!(actual.len(), expected.len() * 3 / 2);
            for v in &expected {
                assert!(
                    actual.iter().any(|a| close(v, a)),
                    "missing native {header:?}: {v:?}"
                );
            }
            for v in &actual {
                assert!(
                    expected.iter().any(|a| close(v, a)),
                    "unexpected native {header:?}: {v:?}"
                );
            }
            count += 1;
        }
        assert_eq!(count, 28);
    }
}
