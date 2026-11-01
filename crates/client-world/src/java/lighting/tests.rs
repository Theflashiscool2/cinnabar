use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};
fn fixture(sky: bool) -> (Engine, Terrain) {
    let states:Vec<_>=(0..5).map(|id|{
        let mut faces:[Vec<[f32;4]>;6]=Default::default();
        if id==3 {faces=[vec![[0.,0.,0.5,1.]],vec![[0.,0.,0.5,1.]],vec![[0.,0.,1.,1.]],vec![],vec![[0.,0.,1.,0.5]],vec![[0.,0.,1.,0.5]]];}
        json!({"id":id,"kind":"empty","height":0.,"falling":false,"solid":id!=0,"solid_render":id==1,"flow_blocking":false,"overlay":false,"invisible":false,"emission":if id==2{15}else{0},"occlusion":([0.0;6]),"full_face":0,"light":{"dampening":if id==1||id==2{15}else if id==4{1}else{0},"faces":faces}})
    }).collect();
    let bytes=serde_json::to_vec(&json!({"schema":3,"light_faces":["west","east","down","up","north","south"],"states":states})).unwrap();
    let facts = Arc::new(Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 5).unwrap());
    let mut engine = Engine::default();
    engine.install(facts).unwrap();
    let mut terrain = Terrain::default();
    terrain.begin(Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:light".into(),
        min_y: -16,
        height: 32,
        has_skylight: sky,
    });
    for x in -1..=1 {
        for z in -1..=1 {
            terrain.replace([x, z], column());
        }
    }
    (engine, terrain)
}
fn column() -> Column {
    Column::new(
        (0..2)
            .map(|_| {
                Arc::new(Section {
                    blocks: Palette::uniform(BlockStateId(0), 4096),
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
    )
}
fn settle(e: &mut Engine, t: &mut Terrain) {
    for _ in 0..256 {
        e.update(t, 2048).unwrap();
        if e.pending() == 0 {
            return;
        }
    }
    panic!("local light did not settle: {} pending", e.pending());
}
#[test]
fn bounded_addition_removal_overlapping_sources_and_occluders() {
    let (mut e, mut t) = fixture(false);
    t.set_block([0, 0, 0], BlockStateId(2));
    e.update(&mut t, 1).unwrap();
    assert_eq!(t.light([0, 0, 0]), Some([15, 0]));
    assert!(e.pending() > 0);
    settle(&mut e, &mut t);
    assert_eq!(t.light([2, 0, 0]), Some([13, 0]));
    t.set_block([1, 0, 0], BlockStateId(1));
    settle(&mut e, &mut t);
    assert_eq!(t.light([1, 0, 0]), Some([0, 0]));
    assert_eq!(t.light([2, 0, 0]), Some([11, 0]));
    t.set_block([1, 0, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([2, 0, 0]), Some([13, 0]));
    t.set_block([4, 0, 0], BlockStateId(2));
    settle(&mut e, &mut t);
    t.set_block([0, 0, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 0, 0]), Some([11, 0]));
    assert_eq!(t.light([4, 0, 0]), Some([15, 0]));
    t.set_block([4, 0, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 0, 0]), Some([0, 0]));
    assert_eq!(t.light([4, 0, 0]), Some([0, 0]));
}
#[test]
fn direct_sky_removal_and_partial_bottom_face_are_distinct_from_block_light() {
    let (mut e, mut t) = fixture(true);
    t.set_block([0, 4, 0], BlockStateId(1));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 4, 0]), Some([0, 0]));
    assert_eq!(t.light([0, 3, 0]), Some([0, 14]));
    t.set_block([0, 4, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 3, 0]), Some([0, 15]));
    t.set_block([0, 4, 0], BlockStateId(3));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 4, 0]), Some([0, 15]));
    assert_eq!(t.light([0, 3, 0]), Some([0, 14]));
    t.set_block([0, 4, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 3, 0]), Some([0, 15]));
}
#[test]
fn negative_section_chunk_boundaries_and_missing_neighbor_recovery() {
    let (mut e, mut t) = fixture(false);
    t.set_block([-1, -1, 0], BlockStateId(2));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 0, 0]), Some([13, 0]));
    t.set_block([-1, -1, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 0, 0]), Some([0, 0]));
    t.unload([1, 0]);
    e.unloaded([1, 0]);
    t.set_block([15, 0, 0], BlockStateId(2));
    settle(&mut e, &mut t);
    assert!(e.waiting() > 0);
    t.replace([1, 0], column());
    e.arrived(&t, [1, 0]);
    settle(&mut e, &mut t);
    assert_eq!(t.light([15, 0, 0]), Some([15, 0]));
    assert_eq!(t.light([16, 0, 0]), Some([14, 0]));
}
#[test]
fn replacement_session_changes_and_authoritative_arrays_do_not_publish_old_work() {
    let (mut e, mut t) = fixture(false);
    t.set_block([0, 0, 0], BlockStateId(2));
    e.update(&mut t, 1).unwrap();
    t.replace([0, 0], column());
    e.arrived(&t, [0, 0]);
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 0, 0]), Some([0, 0]));
    let mut bytes = vec![0; 2048];
    bytes[0] = 7;
    t.update_light(
        [0, 0],
        Light {
            sky: vec![None; 4],
            block: vec![None, None, Some(Arc::from(bytes)), None],
        },
    );
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 0, 0]), Some([7, 0]));
    t.set_block([0, 0, 0], BlockStateId(2));
    e.update(&mut t, 1).unwrap();
    let mut d = t.dimension().unwrap().clone();
    d.session = 8;
    d.epoch = 2;
    t.begin(d);
    t.replace([0, 0], column());
    e.update(&mut t, 1).unwrap();
    assert_eq!(e.pending(), 0);
    assert_eq!(e.waiting(), 0);
    assert_eq!(t.light([0, 0, 0]), Some([0, 0]));
}
#[test]
fn light_storage_boundary_layers_keep_bottom_and_top_faces_lit() {
    let (mut e, mut t) = fixture(false);
    for y in [-16, 15] {
        t.set_block([0, y, 0], BlockStateId(2));
        settle(&mut e, &mut t);
        let outside = if y < 0 { y - 1 } else { y + 1 };
        assert_eq!(t.light([0, outside, 0]), Some([14, 0]));
        t.set_block([0, y, 0], BlockStateId(0));
        settle(&mut e, &mut t);
        assert_eq!(t.light([0, outside, 0]), Some([0, 0]));
    }
}

#[test]
fn attenuation_limits_direct_sky_in_deep_translucent_layers() {
    let (mut e, mut t) = fixture(true);
    for x in -2..=2 {
        for z in -2..=2 {
            for y in [3, 4] {
                t.set_block([x, y, z], BlockStateId(4));
            }
        }
    }
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 4, 0]), Some([0, 14]));
    assert_eq!(t.light([0, 3, 0]), Some([0, 13]));
    t.set_block([0, 4, 0], BlockStateId(0));
    settle(&mut e, &mut t);
    assert_eq!(t.light([0, 3, 0]), Some([0, 14]));
}
