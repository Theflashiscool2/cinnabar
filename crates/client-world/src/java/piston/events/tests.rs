use super::*;
use assets::registry::java::BlockRegistry;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};
use world::java::{BiomeId, Column, Dimension, Light, Palette, Section};
fn fixture() -> Option<(Arc<BlockRegistry>, Terrain)> {
    let Ok(path) = std::env::var("CINNABAR_JAVA_PISTON_BLOCKS") else {
        eprintln!("missing fixture: CINNABAR_JAVA_PISTON_BLOCKS");
        return None;
    };
    static REGISTRY: OnceLock<Arc<BlockRegistry>> = OnceLock::new();
    let r = Arc::clone(REGISTRY.get_or_init(|| {
        Arc::new(
            BlockRegistry::load(
                &std::fs::read(path).unwrap(),
                java_protocol::target::BLOCK_REPORT_SHA256,
            )
            .unwrap(),
        )
    }));
    let table = super::super::facts().unwrap();
    let mut t = Terrain::default();
    t.begin(Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:world".into(),
        min_y: 0,
        height: 32,
        has_skylight: true,
    });
    t.install_block_types(r.block_types());
    for x in -1..=1 {
        for z in -1..=1 {
            t.replace(
                [x, z],
                Column::new(
                    (0..2)
                        .map(|_| {
                            Arc::new(Section {
                                blocks: Palette::uniform(BlockStateId(table.air), 4096),
                                biomes: Palette::uniform(BiomeId(0), 64),
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
        }
    }
    Some((r, t))
}
#[test]
fn extension_uses_original_states_and_authoritative_replacement_retires_motion() {
    let Some((r, mut t)) = fixture() else { return };
    let f = super::super::facts().unwrap();
    let base = f.select(Kind::Base, 5, false, false, false).unwrap();
    let block = f.variant(base).unwrap().block;
    let stone = r.state_id("minecraft:stone", &BTreeMap::new()).unwrap();
    t.set_block([0, 4, 0], BlockStateId(base));
    t.set_block([1, 4, 0], BlockStateId(stone));
    t.set_block([2, 4, 0], BlockStateId(stone));
    assert!(event(&mut t, [0, 4, 0], block, 0, 5).unwrap());
    assert_eq!(t.pistons.len(), 3);
    assert_eq!(t.pistons[&[3, 4, 0]].state, stone);
    assert!(t.pistons[&[1, 4, 0]].source);
    assert!(f.variant(t.block([0, 4, 0]).unwrap().0).unwrap().extended);
    t.set_block([3, 4, 0], BlockStateId(stone));
    assert!(!t.pistons.contains_key(&[3, 4, 0]));
    for _ in 0..9 {
        super::super::tick(&mut t, Some(&r)).unwrap();
    }
    assert_eq!(t.block([3, 4, 0]), Some(BlockStateId(stone)));
    assert!(t.pistons.is_empty());
}
#[test]
fn sticky_retraction_pulls_and_normal_retraction_leaves_the_front_block() {
    let Some((r, mut t)) = fixture() else { return };
    let f = super::super::facts().unwrap();
    let stone = r.state_id("minecraft:stone", &BTreeMap::new()).unwrap();
    for (x, sticky) in [(-1, true), (15, false)] {
        let base = f.select(Kind::Base, 5, sticky, true, false).unwrap();
        let block = f.variant(base).unwrap().block;
        t.set_block([x, 4, 0], BlockStateId(base));
        t.set_block(
            [x + 1, 4, 0],
            BlockStateId(f.select(Kind::Head, 5, sticky, false, false).unwrap()),
        );
        t.set_block([x + 2, 4, 0], BlockStateId(stone));
        assert!(event(&mut t, [x, 4, 0], block, 1, 5).unwrap());
        assert!(t.pistons[&[x, 4, 0]].source);
        if sticky {
            assert_eq!(t.pistons[&[x + 1, 4, 0]].state, stone);
            assert_eq!(t.block([x + 2, 4, 0]), Some(BlockStateId(f.air)));
        } else {
            assert_eq!(t.block([x + 2, 4, 0]), Some(BlockStateId(stone)));
            assert!(!t.pistons.contains_key(&[x + 1, 4, 0]));
        }
    }
}
#[test]
fn duplicate_wrong_identity_unload_and_new_world_do_not_reuse_motion() {
    let Some((_, mut t)) = fixture() else { return };
    let f = super::super::facts().unwrap();
    let base = f.select(Kind::Base, 5, false, false, false).unwrap();
    let block = f.variant(base).unwrap().block;
    t.set_block([-1, 4, 0], BlockStateId(base));
    assert!(!event(&mut t, [-1, 4, 0], block + 1, 0, 5).unwrap());
    assert!(event(&mut t, [-1, 4, 0], block, 0, 5).unwrap());
    assert!(!event(&mut t, [-1, 4, 0], block, 0, 5).unwrap());
    assert_eq!(t.pistons.len(), 1);
    t.unload([0, 0]);
    assert!(t.pistons.is_empty());
    let mut d = t.dimension().unwrap().clone();
    d.epoch += 1;
    t.begin(d);
    assert!(t.pistons.is_empty());
}

#[test]
fn missing_neighbor_waits_atomically_and_never_replays_against_a_later_chunk() {
    let Some((r, mut t)) = fixture() else { return };
    let f = super::super::facts().unwrap();
    let base = f.select(Kind::Base, 5, false, false, false).unwrap();
    let block = f.variant(base).unwrap().block;
    let stone = r.state_id("minecraft:stone", &BTreeMap::new()).unwrap();
    let saved = t.column([1, 0]).unwrap().as_ref().clone();
    t.unload([1, 0]);
    t.set_block([14, 4, 0], BlockStateId(base));
    t.set_block([15, 4, 0], BlockStateId(stone));
    assert!(!event(&mut t, [14, 4, 0], block, 0, 5).unwrap());
    assert_eq!(t.block([14, 4, 0]), Some(BlockStateId(base)));
    assert_eq!(t.block([15, 4, 0]), Some(BlockStateId(stone)));
    assert!(t.pistons.is_empty());
    assert_eq!(t.pending_pistons.len(), 1);
    t.replace([1, 0], saved.clone());
    t.set_block([16, 4, 0], BlockStateId(stone));
    super::super::tick(&mut t, Some(&r)).unwrap();
    assert!(t.pending_pistons.is_empty());
    assert!(t.pistons.is_empty());
    assert_eq!(t.block([16, 4, 0]), Some(BlockStateId(stone)));
    assert_eq!(t.block([17, 4, 0]), Some(BlockStateId(f.air)));

    t.set_block([14, 4, 0], BlockStateId(base));
    t.set_block([15, 4, 0], BlockStateId(stone));
    t.unload([1, 0]);
    assert!(!event(&mut t, [14, 4, 0], block, 0, 5).unwrap());
    t.set_block([15, 4, 0], BlockStateId(f.air));
    t.replace([1, 0], saved);
    super::super::tick(&mut t, Some(&r)).unwrap();
    assert!(t.pending_pistons.is_empty());
    assert_eq!(t.block([14, 4, 0]), Some(BlockStateId(base)));
    assert_eq!(t.block([16, 4, 0]), Some(BlockStateId(f.air)));
}

#[test]
fn same_host_property_updates_keep_snapshot_and_equal_final_writes_retire_it() {
    let Some((r, mut t)) = fixture() else { return };
    let f = super::super::facts().unwrap();
    let base = f.select(Kind::Base, 5, false, false, false).unwrap();
    let block = f.variant(base).unwrap().block;
    t.set_block([0, 4, 0], BlockStateId(base));
    event(&mut t, [0, 4, 0], block, 0, 5).unwrap();
    let p = [1, 4, 0];
    let generation = t.block_generation(p);
    let original = t.pistons[&p].state;
    let host = f.select(Kind::Moving, 4, false, false, false).unwrap();
    t.set_block(p, BlockStateId(host));
    assert_eq!(t.block_generation(p), generation);
    assert_eq!(t.pistons[&p].state, original);
    assert_eq!(t.pistons[&p].facing, 5);
    t.pistons.get_mut(&p).unwrap().state = host;
    for _ in 0..9 {
        super::super::tick(&mut t, Some(&r)).unwrap();
    }
    assert!(t.pistons.is_empty());
    assert_eq!(t.block(p), Some(BlockStateId(host)));
}
