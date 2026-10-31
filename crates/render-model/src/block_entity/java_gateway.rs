//! Native gateway cube and two-material beacon beam; no Bedrock model substitution.
use super::{
    atlas::BlockEntityAtlas,
    mesh::{Layer, MeshBuilder},
    scene::SceneClock,
};
use assets::registry::java::{
    gateway::{BEAM, COOLDOWN_COLOR, LAYERS, SPAWN_COLOR},
    portal::Resources,
};
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct Model {
    pub resources: Arc<Resources>,
    /// Visible faces in shared -X,+X,-Y,+Y,-Z,+Z order.
    pub faces: u8,
    pub beam: Option<Beam>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub height: i32,
    pub scale: f32,
    pub spawning: bool,
}
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.resources, &other.resources)
            && self.faces == other.faces
            && self.beam == other.beam
    }
}
/// Missing data is native age zero. Malformed authority is diagnosed, never executed.
pub fn age(data: Option<&world::NbtValue>) -> Result<i64, String> {
    use world::NbtValue;
    let Some(data) = data else { return Ok(0) };
    let NbtValue::Compound(data) = data else {
        return Err("Java gateway compound expected".into());
    };
    match data.get("Age") {
        None => Ok(0),
        Some(NbtValue::Byte(n)) => Ok(i64::from(*n)),
        Some(NbtValue::Short(n)) => Ok(i64::from(*n)),
        Some(NbtValue::Int(n)) => Ok(i64::from(*n)),
        Some(NbtValue::Long(n)) => Ok(*n),
        Some(NbtValue::Float(n)) => Ok(*n as i64),
        Some(NbtValue::Double(n)) => Ok(*n as i64),
        _ => Err("Java gateway numeric Age expected".into()),
    }
}
pub(super) fn emit(
    b: &mut MeshBuilder,
    atlas: &BlockEntityAtlas,
    block: [i32; 3],
    m: &Model,
    clock: SceneClock,
) {
    let p = block.map(|v| v as f32);
    for (face, corners) in assets::block_entity_geometry::tile_box_faces([[0.; 3], [1.; 3]])
        .into_iter()
        .enumerate()
    {
        if m.faces & (1 << face) == 0 {
            continue;
        }
        let mut corners = corners.map(|v| std::array::from_fn(|i| v[i] + p[i]));
        // Shared box helpers use inward winding; the portal pass culls back faces.
        corners.reverse();
        b.quad_uv(
            Layer::Portal,
            corners,
            [[0.; 2]; 4],
            [0., 0., 0., -(LAYERS as f32)],
        );
    }
    let Some(beam) = m.beam.filter(|v| v.height > 0) else {
        return;
    };
    let Some(texture) = b.required_texture(atlas, BEAM, [16.; 2]) else {
        return;
    };
    let rgb = if beam.spawning {
        SPAWN_COLOR
    } else {
        COOLDOWN_COLOR
    };
    let color = [
        ((rgb >> 16) & 255) as f32 / 255.,
        ((rgb >> 8) & 255) as f32 / 255.,
        (rgb & 255) as f32 / 255.,
    ];
    let t = clock.ticks.rem_euclid(40.) as f32;
    let scroll = (-t * 0.2 - (-t * 0.1).floor()).rem_euclid(1.);
    let angle = (t * 2.25 - 45.).to_radians();
    let (s, c) = angle.sin_cos();
    for (layer, radius, alpha) in [
        (Layer::Solid, 0.15, 1.),
        (Layer::Overlay, 0.175, 32. / 255.),
    ] {
        let points = if layer == Layer::Solid {
            [[0., radius], [radius, 0.], [0., -radius], [-radius, 0.]]
                .map(|[x, z]| [x * c + z * s, -x * s + z * c])
        } else {
            [
                [-radius, -radius],
                [radius, -radius],
                [radius, radius],
                [-radius, radius],
            ]
        };
        let low = p[1] - beam.height as f32;
        let high = p[1] + beam.height as f32;
        let bottom = -1. + scroll;
        let top = bottom
            + (2. * beam.height as f32)
                * beam.scale
                * if layer == Layer::Solid {
                    0.5 / radius
                } else {
                    1.
                };
        for i in 0..4 {
            let a = points[i];
            let d = points[(i + 1) % 4];
            let corners = [
                [a[0] + p[0] + 0.5, high, a[1] + p[2] + 0.5],
                [d[0] + p[0] + 0.5, high, d[1] + p[2] + 0.5],
                [d[0] + p[0] + 0.5, low, d[1] + p[2] + 0.5],
                [a[0] + p[0] + 0.5, low, a[1] + p[2] + 0.5],
            ];
            b.java_beam_quad(
                layer,
                corners,
                texture.rect,
                [color[0], color[1], color[2], alpha],
                [top, bottom],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn atlas() -> BlockEntityAtlas {
        use assets::{BlockEntityPlacement, RuntimeBlockEntityAssets, encode_block_entity_catalog};
        let bytes = encode_block_entity_catalog(
            b"{}",
            16,
            16,
            &vec![255; 16 * 16 * 4],
            &[BlockEntityPlacement {
                name: BEAM.into(),
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            }],
        )
        .unwrap();
        BlockEntityAtlas::from_assets(&RuntimeBlockEntityAssets::decode(&bytes).unwrap())
    }
    #[test]
    fn native_cube_face_mask_and_beam_materials_heights_uvs() {
        let atlas = atlas();
        let mut b = MeshBuilder::new(atlas.size());
        let mut m = Model {
            resources: Arc::new(Resources {
                colors: [[0.; 4]; 16],
            }),
            faces: 63,
            beam: None,
        };
        emit(&mut b, &atlas, [-17, 38, 16], &m, SceneClock { ticks: 20. });
        assert_eq!(b.portal.len(), 36);
        let center = glam::Vec3::new(-16.5, 38.5, 16.5);
        for tri in b.portal.chunks_exact(3) {
            let p = tri
                .iter()
                .map(|v| glam::Vec3::from(v.position))
                .collect::<Vec<_>>();
            assert!((p[1] - p[0]).cross(p[2] - p[0]).dot(p[0] - center) > 0.);
            assert!(tri.iter().all(|v| v.color[3] == -16.));
        }
        m.faces = 1 << 3;
        m.beam = Some(Beam {
            height: 50,
            scale: 1.,
            spawning: false,
        });
        let mut b = MeshBuilder::new(atlas.size());
        emit(&mut b, &atlas, [0, 40, 0], &m, SceneClock { ticks: 20. });
        assert_eq!(b.portal.len(), 6);
        assert!(b.portal.iter().all(|v| v.position[1] == 41.));
        assert_eq!(b.solid.len(), 24);
        assert_eq!(b.overlay.len(), 24);
        for v in &b.solid {
            let dx = v.position[0] - 0.5;
            let dz = v.position[2] - 0.5;
            assert!((dx.hypot(dz) - 0.15).abs() < 1e-6);
            assert!(v.position[1] == 90. || v.position[1] == -10.);
            let expected = if v.position[1] == 90. { 332.33334 } else { -1. };
            assert!((v.uv[1] - expected).abs() < 0.0001);
            assert_eq!(v.color[3], 1.);
            assert_eq!(v.actor_light, u32::MAX - 1);
        }
        for v in &b.overlay {
            assert!(((v.position[0] - 0.5).abs() - 0.175).abs() < 1e-6);
            assert!(((v.position[2] - 0.5).abs() - 0.175).abs() < 1e-6);
            assert_eq!(v.color[3], 32. / 255.);
            assert_eq!(v.uv[1], if v.position[1] == 90. { 99. } else { -1. });
        }
        m.beam = None;
        let mut retired = MeshBuilder::new(atlas.size());
        emit(&mut retired, &atlas, [0, 40, 0], &m, SceneClock::default());
        assert!(retired.solid.is_empty() && retired.overlay.is_empty());
    }
    #[test]
    fn authoritative_age_is_bounded_and_not_inferred_from_position() {
        use world::NbtValue;
        assert_eq!(age(None).unwrap(), 0);
        for (v, expected) in [
            (NbtValue::Long(123), Some(123)),
            (NbtValue::Long(-1), Some(-1)),
            (NbtValue::Int(2), Some(2)),
        ] {
            let mut compound = world::NbtCompound::default();
            compound.insert("Age", v);
            let data = NbtValue::Compound(compound);
            assert_eq!(age(Some(&data)).ok(), expected);
        }
        assert!(age(Some(&NbtValue::String("not executable".into()))).is_err());
    }
    #[test]
    fn matches_independent_native_beacon_renderer_vertices() {
        let Ok(path) = std::env::var("CINNABAR_JAVA_GATEWAY_VERTICES") else {
            eprintln!("missing CINNABAR_JAVA_GATEWAY_VERTICES fixture; native comparison skipped");
            return;
        };
        let text = std::fs::read_to_string(path).unwrap();
        let mut lines = text.lines();
        let atlas = atlas();
        let mut count = 0;
        while let Some(line) = lines.next() {
            let Some(id) = line.strip_prefix("CASE ") else {
                continue;
            };
            let id: usize = id.parse().unwrap();
            let layer = count % 2;
            let expected: Vec<[f32; 5]> = lines
                .by_ref()
                .take_while(|l| *l != "END")
                .map(|l| {
                    l.split_whitespace()
                        .map(|s| s.parse::<f32>().unwrap())
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap()
                })
                .collect();
            assert_eq!(expected.len(), 16);
            let mut b = MeshBuilder::new(atlas.size());
            let model = Model {
                resources: Arc::new(Resources {
                    colors: [[0.; 4]; 16],
                }),
                faces: 0,
                beam: Some(Beam {
                    height: 50,
                    scale: [1., 0.5, 0.25][id],
                    spawning: false,
                }),
            };
            emit(
                &mut b,
                &atlas,
                [0; 3],
                &model,
                SceneClock {
                    ticks: [20., 0., 37.5][id],
                },
            );
            let actual: Vec<[f32; 5]> = if layer == 0 { b.solid } else { b.overlay }
                .iter()
                .map(|v| {
                    [
                        v.position[0],
                        v.position[1],
                        v.position[2],
                        v.uv[0],
                        v.uv[1],
                    ]
                })
                .collect();
            let close =
                |a: &[f32; 5], b: &[f32; 5]| a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.0002);
            assert!(
                actual.iter().all(|a| expected.iter().any(|b| close(a, b))),
                "case{id} layer{layer} emitted mismatch"
            );
            assert!(
                expected.iter().all(|a| actual.iter().any(|b| close(a, b))),
                "case{id} layer{layer} missing native vertex"
            );
            count += 1;
        }
        assert_eq!(count, 6);
    }
}
