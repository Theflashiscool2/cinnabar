//! PistonMovingBlockEntity.getCollisionShape for the pinned Java runtime.
//! Uses exported physics shapes, never visual-model bounds. Entity pushing is a separate step.
use assets::registry::java::{
    physics::{Bounds, Collision, Table},
    piston::{BASE, HEAD, MOVING, STICKY_BASE},
};
use world::java::Terrain;

/// None preserves the caller's explicit unsupported-context error. No snapshot is empty air.
pub fn shape(terrain: &Terrain, physics: &Table, position: [i32; 3]) -> Option<Vec<Bounds>> {
    let registry = physics.registry()?;
    let host = terrain.block(position)?;
    if registry.state(host.0)?.name.as_ref() != MOVING {
        return None;
    }
    let moving = terrain.pistons.get(&position)?;
    if terrain.block_generation(position) != Some(moving.generation) {
        return None;
    }
    let state = registry.state(moving.state)?;
    let progress = moving.motion.progress(1.);
    let mut result = Vec::new();
    let source_base = moving.source && !moving.extending;
    if source_base {
        if !matches!(state.name.as_ref(), BASE | STICKY_BASE) {
            return None;
        }
        let mut properties = state.properties.clone();
        properties.insert("extended".into(), "true".into());
        let base = registry.state_id(&state.name, &properties)?;
        result.extend_from_slice(static_shape(physics, base)?);
    }
    let id = if moving.source {
        let facing = world::java::piston::facing_name(moving.facing)?;
        // Native collision's quarter-block threshold is distinct from its rendered head.
        let properties = std::collections::BTreeMap::from([
            ("facing".into(), facing.into()),
            (
                "short".into(),
                (moving.extending != (1. - progress < 0.25)).to_string(),
            ),
        ]);
        registry.state_id(HEAD, &properties)?
    } else {
        moving.state
    };
    let offset = moving
        .motion
        .offset(moving.facing, moving.extending, 1.)?
        .map(f64::from);
    for b in static_shape(physics, id)? {
        result.push(Bounds {
            min: std::array::from_fn(|i| b.min[i] + offset[i]),
            max: std::array::from_fn(|i| b.max[i] + offset[i]),
        });
    }
    Some(result)
}
fn static_shape(physics: &Table, id: u32) -> Option<&[Bounds]> {
    match &physics.state(id)?.collision {
        Collision::Static(shape) => Some(shape),
        Collision::Dynamic => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use world::java::{
        BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section,
        piston::{Motion, Moving},
    };
    #[derive(serde::Deserialize)]
    struct Case {
        facing: u8,
        extending: bool,
        source: bool,
        state: u32,
        progress: f32,
        boxes: Vec<[f64; 6]>,
    }
    #[test]
    fn contextual_union_matches_288_pinned_native_collision_shapes() {
        let (Ok(root), Ok(reference)) = (
            std::env::var("CINNABAR_JAVA_TEST_ASSETS"),
            std::env::var("CINNABAR_JAVA_PISTON_COLLISION"),
        ) else {
            eprintln!(
                "missing fixture: CINNABAR_JAVA_TEST_ASSETS and CINNABAR_JAVA_PISTON_COLLISION"
            );
            return;
        };
        let load = |name| std::fs::read(std::path::Path::new(&root).join(name)).unwrap();
        let blocks = load("blocks.json");
        let digest = |bytes: &[u8]| {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(bytes))
        };
        let registry = Arc::new(
            assets::registry::java::BlockRegistry::load(&blocks, &digest(&blocks)).unwrap(),
        );
        let physics_bytes = load("physics.bin");
        let physics = Table::load(&physics_bytes, &digest(&physics_bytes), registry.len())
            .unwrap()
            .with_registry(Arc::clone(&registry))
            .unwrap();
        let host = registry.state_id(MOVING, &Default::default()).unwrap();
        let mut terrain = Terrain::default();
        terrain.begin(Dimension {
            session: 1,
            epoch: 1,
            name: "fixture:piston".into(),
            min_y: 0,
            height: 16,
            has_skylight: false,
        });
        terrain.replace(
            [0, 0],
            Column::new(
                vec![Arc::new(Section {
                    blocks: Palette::uniform(BlockStateId(host), 4096),
                    biomes: Palette::uniform(BiomeId(0), 64),
                })],
                Light {
                    block: vec![None; 3],
                    sky: vec![None; 3],
                },
                vec![],
                vec![],
            ),
        );
        let mut cases = 0;
        for line in std::fs::read_to_string(reference).unwrap().lines() {
            let Ok(case) = serde_json::from_str::<Case>(
                line.get(line.find('{').unwrap_or(line.len())..)
                    .unwrap_or(""),
            ) else {
                continue;
            };
            terrain.pistons.insert(
                [0; 3],
                Moving {
                    state: case.state,
                    facing: case.facing,
                    extending: case.extending,
                    source: case.source,
                    motion: Motion::new(case.progress).unwrap(),
                    generation: terrain.block_generation([0; 3]).unwrap(),
                },
            );
            let actual = shape(&terrain, &physics, [0; 3]).unwrap();
            let expected: Vec<_> = case
                .boxes
                .iter()
                .map(|v| Bounds {
                    min: [v[0], v[1], v[2]],
                    max: [v[3], v[4], v[5]],
                })
                .collect();
            // Compare occupied intervals, independent of the native union's box merging/order.
            let axes: [Vec<f64>; 3] = std::array::from_fn(|axis| {
                let mut points = vec![-2., 3.];
                for b in actual.iter().chain(&expected) {
                    points.extend([b.min[axis], b.max[axis]]);
                }
                points.sort_by(f64::total_cmp);
                points.dedup();
                points.windows(2).map(|w| (w[0] + w[1]) * 0.5).collect()
            });
            let contains = |boxes: &[Bounds], p: [f64; 3]| {
                boxes
                    .iter()
                    .any(|b| (0..3).all(|i| b.min[i] < p[i] && p[i] < b.max[i]))
            };
            for &x in &axes[0] {
                for &y in &axes[1] {
                    for &z in &axes[2] {
                        assert_eq!(
                            contains(&actual, [x, y, z]),
                            contains(&expected, [x, y, z]),
                            "{line} at {x},{y},{z}"
                        );
                    }
                }
            }
            cases += 1;
        }
        assert_eq!(cases, 288);
        terrain.pistons.clear();
        assert!(shape(&terrain, &physics, [0; 3]).is_none());
    }
}
