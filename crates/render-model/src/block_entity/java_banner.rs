//! Java banner geometry, native dye/pattern layers and position-dependent cloth motion.
use super::{
    SceneClock,
    atlas::BlockEntityAtlas,
    java_cube::{cuboid, faces},
    mesh::{Layer, MeshBuilder},
};
use assets::{
    block_entity_geometry::BoxSpec,
    registry::java::{
        banner::{BASE, Banner, COLOR_BASE, DYES},
        source::Identifier,
    },
};
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};
use world::NbtValue;
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub texture: String,
    pub color: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub state: Banner,
    pub patterns: Arc<[Pattern]>,
}
impl Model {
    /// Modern Java BannerPatternLayers NBT. Vanilla draws at most the first sixteen layers.
    /// Missing data is a plain banner; malformed/unknown content is an explicit diagnostic.
    pub fn from_data(
        state: Banner,
        data: Option<&NbtValue>,
        registry: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut patterns = Vec::new();
        if let Some(data) = data {
            let NbtValue::Compound(data) = data else {
                return Err("Java banner compound expected".into());
            };
            if let Some(value) = data.get("patterns") {
                let NbtValue::List(layers) = value else {
                    return Err("Java banner pattern list expected".into());
                };
                for layer in layers.iter().take(16) {
                    let NbtValue::Compound(layer) = layer else {
                        return Err("Java banner pattern compound expected".into());
                    };
                    let color = layer
                        .string("color")
                        .and_then(|c| DYES.iter().find(|(n, _)| *n == c))
                        .ok_or("Java banner dye unknown")?
                        .1;
                    let texture = match layer.get("pattern") {
                        Some(NbtValue::String(key)) => {
                            registry.get(key.as_ref()).cloned().ok_or_else(|| {
                                format!(
                                    "Java banner pattern unsupported: {}",
                                    key.chars().take(128).collect::<String>()
                                )
                            })?
                        }
                        Some(NbtValue::Compound(inline)) => {
                            let id = Identifier::parse(
                                inline
                                    .string("asset_id")
                                    .ok_or("Java banner pattern asset missing")?,
                            )
                            .map_err(|e| e.to_string())?;
                            let texture =
                                format!("{}:textures/entity/banner/{}", id.namespace(), id.path());
                            if !registry.values().any(|s| s == &texture) {
                                return Err(format!("Java banner artwork unavailable: {id}"));
                            }
                            texture
                        }
                        _ => return Err("Java banner pattern holder expected".into()),
                    };
                    patterns.push(Pattern { texture, color });
                }
            }
        }
        Ok(Self {
            state,
            patterns: patterns.into(),
        })
    }
}
fn tint(rgb: u32) -> [f32; 4] {
    let linear = |shift| {
        let v = ((rgb >> shift) & 255u32) as f32 / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    [linear(16), linear(8), linear(0), 1.]
}
fn cloth_angle(block: [i32; 3], ticks: f64) -> f32 {
    let seed = block[0]
        .wrapping_mul(7)
        .wrapping_add(block[1].wrapping_mul(9))
        .wrapping_add(block[2].wrapping_mul(13));
    let phase = ((ticks + f64::from(seed)).rem_euclid(100.) / 100.) as f32;
    (-0.0125 + 0.01 * (std::f32::consts::TAU * phase).cos()) * std::f32::consts::PI
}
pub(super) fn emit(
    builder: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    model: &Model,
    clock: SceneClock,
) {
    let Some(texture) = builder.required_texture(atlas, BASE, [64.; 2]) else {
        return;
    };
    let base =
        Mat4::from_translation(Vec3::from_array(block.map(|x| x as f32)) + Vec3::new(0.5, 0., 0.5))
            * Mat4::from_rotation_y(-(model.state.rotation as f32 * 22.5).to_radians())
            * Mat4::from_scale(Vec3::new(1., -1., -1.) * (2. / 3. / 16.));
    let wall = model.state.wall;
    if !wall {
        cuboid(
            builder,
            &texture,
            base,
            BoxSpec::new([-1., -42., -1.], [2., 42., 2.], [44., 0.]),
            63,
        );
    }
    cuboid(
        builder,
        &texture,
        base,
        BoxSpec::new(
            [
                -10.,
                if wall { -20.5 } else { -44. },
                if wall { 9.5 } else { -1. },
            ],
            [20., 2., 2.],
            [0., 42.],
        ),
        63,
    );
    let cloth =
        base * Mat4::from_translation(Vec3::new(
            0.,
            if wall { -20.5 } else { -44. },
            if wall { 10.5 } else { 0. },
        )) * Mat4::from_rotation_x(cloth_angle(block, clock.ticks));
    let flag = BoxSpec::new([-10., 0., -2.], [20., 40., 1.], [0., 0.]);
    cuboid(builder, &texture, cloth, flag, 63);
    // Exact coplanar UVs, submitted in order with alpha blending and equal-depth testing;
    // no inflated shells, missing-glyph stand-ins, or depth-writing pattern cutouts.
    for (name, color) in std::iter::once((COLOR_BASE, model.state.color))
        .chain(model.patterns.iter().map(|p| (p.texture.as_str(), p.color)))
    {
        let Some(texture) = builder.required_texture(atlas, name, [64.; 2]) else {
            return;
        };
        for (corners, texels) in faces(flag) {
            builder.quad(
                Layer::Overlay,
                corners.map(|p| cloth.transform_point3(Vec3::from_array(p)).to_array()),
                texture.rect_uv(texels),
                tint(color),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_entity::{
        BlockEntityKind, BlockEntityLight, BlockEntityScene, BlockEntitySubmission,
    };
    fn layer(pattern: &str, color: &str) -> NbtValue {
        let mut c = world::NbtCompound::default();
        c.insert("pattern", NbtValue::String(pattern.into()));
        c.insert("color", NbtValue::String(color.into()));
        NbtValue::Compound(c)
    }
    fn data(layers: Vec<NbtValue>) -> NbtValue {
        let mut c = world::NbtCompound::default();
        c.insert("patterns", NbtValue::List(layers));
        NbtValue::Compound(c)
    }
    fn state(wall: bool, rotation: u8) -> Banner {
        Banner {
            wall,
            rotation,
            color: 0xb02e26,
        }
    }
    #[test]
    fn authoritative_layers_keep_order_and_reject_unknown_or_malformed_values() {
        let registry = BTreeMap::from([(
            "minecraft:cross".into(),
            "minecraft:textures/entity/banner/cross".into(),
        )]);
        let m = Model::from_data(
            state(false, 0),
            Some(&data(vec![
                layer("minecraft:cross", "white"),
                layer("minecraft:cross", "black"),
            ])),
            &registry,
        )
        .unwrap();
        assert_eq!(
            m.patterns.iter().map(|p| p.color).collect::<Vec<_>>(),
            [0xf9fffe, 0x1d1d21]
        );
        assert_eq!(
            Model::from_data(
                state(false, 0),
                Some(&data(vec![layer("minecraft:cross", "red"); 40])),
                &registry
            )
            .unwrap()
            .patterns
            .len(),
            16
        );
        assert!(
            Model::from_data(
                state(false, 0),
                Some(&data(vec![layer("../bad", "red")])),
                &registry
            )
            .is_err()
        );
        assert!(
            Model::from_data(
                state(false, 0),
                Some(&data(vec![layer("minecraft:cross", "unknown")])),
                &registry
            )
            .is_err()
        );
        assert!(Model::from_data(state(false, 0), Some(&NbtValue::Int(3)), &registry).is_err());
        assert!(
            Model::from_data(state(false, 0), None, &registry)
                .unwrap()
                .patterns
                .is_empty()
        );
    }
    fn scene() -> BlockEntityScene {
        let images = [BASE, COLOR_BASE, "minecraft:textures/entity/banner/cross"]
            .into_iter()
            .map(|n| {
                (
                    n.into(),
                    Arc::new(assets::registry::java::textures::Image {
                        width: 64,
                        height: 64,
                        rgba: vec![255; 64 * 64 * 4],
                    }),
                )
            })
            .collect();
        let mut s = BlockEntityScene::default();
        s.install_assets(
            &assets::registry::java::special::atlas(&images)
                .unwrap()
                .unwrap(),
        );
        s
    }
    fn sub(position: [i32; 3], state: Banner, patterns: Arc<[Pattern]>) -> BlockEntitySubmission {
        BlockEntitySubmission {
            block: position,
            light: BlockEntityLight::Actor { block: 0, sky: 15 },
            kind: BlockEntityKind::JavaBanner(Model { state, patterns }),
        }
    }
    #[test]
    fn reference_mounts_rotations_and_coplanar_layers_preserve_geometry() {
        let mut s = scene();
        for rotation in 0..16 {
            for wall in [false, true] {
                if wall && rotation % 4 != 0 {
                    continue;
                }
                let patterns = Arc::from([Pattern {
                    texture: "minecraft:textures/entity/banner/cross".into(),
                    color: 0xffffff,
                }]);
                let f = s.update(
                    SceneClock { ticks: 0. },
                    &[],
                    &[sub([0; 3], state(wall, rotation), patterns)],
                );
                assert_eq!(f.solid.len(), if wall { 72 } else { 108 });
                assert_eq!(f.overlay.len(), 72);
                // The last support/base draw and both pattern layers share exact positions.
                for i in 0..36 {
                    assert_eq!(
                        f.solid[f.solid.len() - 36 + i].position,
                        f.overlay[i].position
                    );
                    assert_eq!(f.overlay[i].position, f.overlay[36 + i].position);
                }
                let top = f.solid[..f.solid.len() - 36]
                    .iter()
                    .map(|v| v.position[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!((top - if wall { 20.5 / 24. } else { 44. / 24. }).abs() < 1e-5);
                assert!(
                    f.solid.iter().chain(f.overlay.iter()).all(|v| v
                        .position
                        .iter()
                        .chain(&v.uv)
                        .all(|x| x.is_finite()))
                );
            }
        }
        assert!(s.update(SceneClock::default(), &[], &[]).solid.is_empty());
    }
    #[test]
    fn motion_is_periodic_at_negative_positions_and_bounded() {
        for t in 0..100 {
            let a = cloth_angle([-17, 37, -1], t as f64);
            assert!((a - cloth_angle([-17, 37, -1], t as f64 + 100.)).abs() < 1e-6);
            assert!(
                (-0.0225 * std::f32::consts::PI - 1e-6..=-0.0025 * std::f32::consts::PI + 1e-6)
                    .contains(&a)
            );
        }
        assert!(tint(0xffffff)[0] > 0.99);
        assert!(tint(0x1d1d21)[0] < 0.02);
    }
}
