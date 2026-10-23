use super::*;
use assets::registry::java::{
    BlockRegistry,
    catalog::BlockModels,
    source::{Error as AssetError, Source},
};
use sha2::{Digest, Sha256};
use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
struct Air;
impl Source for Air {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, AssetError> {
        match path {
            "assets/minecraft/blockstates/air.json" => {
                Ok(br#"{"variants":{"":{"model":"block/air"}}}"#.to_vec())
            }
            "assets/minecraft/models/block/air.json" => Ok(br#"{"elements":[]}"#.to_vec()),
            _ => Err(AssetError::Model(format!(
                "unexpected fixture resource {path}"
            ))),
        }
    }
}
fn fixture() -> (Terrain, Arc<Graphics>) {
    let report = br#"{"minecraft:air":{"states":[{"id":0,"default":true}]}}"#;
    let registry = BlockRegistry::load(report, &format!("{:x}", Sha256::digest(report))).unwrap();
    let graphics = assets::registry::java::graphics::compile(
        &mut BlockModels::new(Arc::new(registry), Box::new(Air)),
        [0],
        64,
    )
    .unwrap();
    let mut terrain = Terrain::default();
    terrain.begin(Dimension {
        session: 2,
        epoch: 3,
        name: "fixture:world".into(),
        min_y: -16,
        height: 16,
        has_skylight: true,
    });
    for x in -1..=1 {
        for z in -1..=1 {
            terrain.replace(
                [x, z],
                Column::new(
                    vec![Arc::new(Section {
                        blocks: Palette::uniform(BlockStateId(0), 4096),
                        biomes: Palette::uniform(BiomeId(41), 64),
                    })],
                    Light {
                        sky: vec![Some(Arc::from([])); 3],
                        block: vec![Some(Arc::from([])); 3],
                    },
                    vec![],
                    vec![],
                ),
            );
        }
    }
    (terrain, Arc::new(graphics))
}
#[test]
fn native_workers_are_bounded_and_keep_authoritative_biome_indices() {
    let (mut terrain, graphics) = fixture();
    let mut stream = Stream::new(&terrain, graphics).unwrap();
    let mut count = 0;
    test_time::eventually("all native section results", || {
        stream.poll(&mut terrain);
        assert!(stream.active.len() + stream.ready.len() <= WORKERS);
        while let Some(ready) = stream.pop_ready(&terrain) {
            assert!(terrain.is_current(&ready.key));
            assert!(ready.mesh.model_refs().is_empty());
            assert_eq!(ready.biome.uniform_tint_index(), Some(41));
            assert_eq!(ready.biome.tint_index_at([-1, 0, 17]), Some(41));
            count += 1;
        }
        count == 9
    });
    assert!(stream.errors().is_empty());
    assert_eq!(stream.metrics.dispatched, 9);
}
#[test]
fn queued_output_is_invalid_after_dimension_replacement_and_drop_cancels_workers() {
    let (mut terrain, graphics) = fixture();
    let mut stream = Stream::new(&terrain, graphics).unwrap();
    test_time::eventually("queued native mesh", || {
        stream.poll(&mut terrain);
        !stream.ready.is_empty()
    });
    let cancel = Arc::clone(&stream.cancel);
    let mut next = terrain.dimension().unwrap().clone();
    next.epoch += 1;
    terrain.begin(next);
    assert!(stream.pop_ready(&terrain).is_none());
    stream.poll(&mut terrain);
    assert_eq!(stream.pending.len(), 0);
    assert!(cancel.load(Ordering::Acquire));
    drop(stream);
    assert!(cancel.load(Ordering::Acquire));
}
#[test]
fn nonuniform_biomes_are_an_explicit_incomplete_feature() {
    let (mut terrain, _) = fixture();
    let mut palette = Palette::uniform(BiomeId(41), 64);
    palette.set(63, BiomeId(3));
    terrain.update_biomes([1, 1], vec![palette]);
    let snapshot = Snapshot::capture(&terrain, [0, 0]).unwrap();
    assert!(
        uniform_biome(&snapshot, -1)
            .unwrap_err()
            .to_string()
            .contains("Voronoi")
    );
}
