//! Independent vectors emitted by ProbeFluidCurrents against the verified official runtime.
use super::*;
use assets::registry::java::{BlockRegistry, fluids};
use std::{path::PathBuf, sync::Arc};
use world::java::{BlockStateId, Column, Light, Palette, Section};

#[test]
fn native_fluid_currents_match_official_runtime_and_negative_translation() {
    let Some(root) = std::env::var_os("CINNABAR_JAVA_ASSETS").map(PathBuf::from) else {
        eprintln!(
            "SKIP native fluid current reference: CINNABAR_JAVA_ASSETS verified runtime missing"
        );
        return;
    };
    let pin: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../java-protocol/data/target.json"
    ))
    .unwrap();
    let count = pin["block_registry_report"]["states"].as_u64().unwrap() as usize;
    let registry = Arc::new(
        BlockRegistry::load(
            &std::fs::read(root.join("blocks.json")).unwrap(),
            pin["block_registry_report"]["sha256"].as_str().unwrap(),
        )
        .unwrap(),
    );
    let physics = Table::load(
        &std::fs::read(root.join("physics.bin")).unwrap(),
        pin["physics_report"]["sha256"].as_str().unwrap(),
        count,
    )
    .unwrap()
    .with_registry(registry)
    .unwrap();
    let fluid = fluids::Table::load(
        &std::fs::read(root.join("fluids.json")).unwrap(),
        pin["fluid_render_report"]["sha256"].as_str().unwrap(),
        count,
    )
    .unwrap();
    let physics = physics.with_fluids(Arc::new(fluid)).unwrap();
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-fluid-currents.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        for shift in [[0_i32, 0, 0], [-16, 0, -16], [8, 16, 8]] {
            let mut terrain = Terrain::default();
            terrain.begin(world::java::Dimension {
                session: 7,
                epoch: 1,
                name: "fixture:current".into(),
                min_y: -32,
                height: 64,
                has_skylight: true,
            });
            let center = [(8 + shift[0]).div_euclid(16), (8 + shift[2]).div_euclid(16)];
            for x in center[0] - 1..=center[0] + 1 {
                for z in center[1] - 1..=center[1] + 1 {
                    terrain.replace(
                        [x, z],
                        Column::new(
                            (0..4)
                                .map(|_| {
                                    Arc::new(Section {
                                        blocks: Palette::uniform(BlockStateId(0), 4096),
                                        biomes: Palette::uniform(world::java::BiomeId(0), 64),
                                    })
                                })
                                .collect(),
                            Light {
                                sky: vec![None; 6],
                                block: vec![None; 6],
                            },
                            vec![],
                            vec![],
                        ),
                    );
                }
            }
            for cell in case["blocks"].as_array().unwrap() {
                let p = std::array::from_fn(|i| cell[i].as_i64().unwrap() as i32 + shift[i]);
                terrain.set_block(p, BlockStateId(cell[3].as_u64().unwrap() as u32));
                assert_eq!(
                    terrain.block(p),
                    Some(BlockStateId(cell[3].as_u64().unwrap() as u32))
                );
            }
            let p = [8 + shift[0], shift[1], 8 + shift[2]];
            let id = terrain.block(p).unwrap().0;
            let cells = super::super::liquid::Cells {
                terrain: &terrain,
                physics: &physics,
            };
            let actual = cells
                .flow(physics.fluids().unwrap().state(id).unwrap(), p)
                .unwrap();
            let expected = std::array::from_fn(|i| case["flow"][i].as_f64().unwrap());
            assert_eq!(actual, expected, "{} at {shift:?}", case["name"]);
        }
    }
    let facts: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-swimming.json"
    ))
    .unwrap();
    for case in facts["camera_cases"].as_array().unwrap() {
        for [x, y, z] in [[8_i32, 0, 8], [-16, 15, -16]] {
            let mut t = Terrain::default();
            t.begin(world::java::Dimension {
                session: 7,
                epoch: 1,
                name: "fixture:camera".into(),
                min_y: 0,
                height: 32,
                has_skylight: true,
            });
            t.replace(
                [x.div_euclid(16), z.div_euclid(16)],
                Column::new(
                    (0..2)
                        .map(|_| {
                            Arc::new(Section {
                                blocks: Palette::uniform(BlockStateId(0), 4096),
                                biomes: Palette::uniform(world::java::BiomeId(0), 64),
                            })
                        })
                        .collect(),
                    Light {
                        sky: vec![None; 4],
                        block: vec![None; 4],
                    },
                    vec![],
                    vec![],
                ),
            );
            t.set_block(
                [x, y, z],
                BlockStateId(case["water"].as_u64().unwrap() as u32),
            );
            t.set_block(
                [x, y + 1, z],
                BlockStateId(case["above"].as_u64().unwrap() as u32),
            );
            let cells = super::super::liquid::Cells {
                terrain: &t,
                physics: &physics,
            };
            let height = f64::from(case["height"].as_f64().unwrap() as f32);
            for delta in [-1.0e-6, 1.0e-6] {
                assert_eq!(
                    cells
                        .water_at_eye(
                            [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5],
                            height + delta
                        )
                        .unwrap(),
                    delta < 0.,
                    "{} at {:?}",
                    case["name"],
                    [x, y, z]
                );
            }
        }
    }
}
