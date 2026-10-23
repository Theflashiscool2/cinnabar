use super::*;
use world::java::{BiomeId, BlockStateId, Light, Palette, Section, Terrain};
fn terrain() -> Terrain {
    let mut terrain = Terrain::default();
    terrain.begin(Dimension {
        session: 7,
        epoch: 2,
        name: "example:negative".into(),
        min_y: -64,
        height: 16,
        has_skylight: true,
    });
    let blocks = Palette::uniform(BlockStateId(42), 4096);
    let section = Arc::new(Section {
        blocks,
        biomes: Palette::uniform(BiomeId(2), 64),
    });
    let mut sky = vec![0u8; 2048];
    sky[0] = 0xA3;
    sky[2047] = 0xC5;
    terrain.replace(
        [-1, -2],
        Column::new(
            vec![section],
            Light {
                sky: vec![Some(Arc::from([])), Some(Arc::from(sky)), None],
                block: vec![
                    Some(Arc::from([])),
                    Some(Arc::from(vec![0x72; 2048])),
                    Some(Arc::from([])),
                ],
            },
            vec![],
            vec![],
        ),
    );
    terrain
}
#[test]
fn negative_coordinates_light_nibbles_and_unknown_boundary_are_distinct() {
    let terrain = terrain();
    let snapshot = Snapshot::capture(&terrain, [-1, -2]).unwrap();
    assert_eq!(snapshot.block([-16, -64, -32]), Some(42));
    assert_eq!(snapshot.light([-16, -64, -32]), Some([2, 3]));
    assert_eq!(snapshot.light([-15, -64, -32]), Some([7, 10]));
    assert_eq!(snapshot.light([-1, -49, -17]), Some([7, 12]));
    assert_eq!(snapshot.light([-16, -65, -32]), Some([0, 0]));
    assert_eq!(snapshot.light([-16, -48, -32]), Some([0, 15]));
    assert_eq!(snapshot.light([0, -64, -32]), None);
    assert_eq!(snapshot.block([-16, -65, -32]), None);
}
#[test]
fn worker_snapshot_keeps_old_authority_but_cannot_publish_after_replacement() {
    let mut terrain = terrain();
    let snapshot = Snapshot::capture(&terrain, [-1, -2]).unwrap();
    terrain.set_block([-16, -64, -32], BlockStateId(100));
    assert_eq!(snapshot.block([-16, -64, -32]), Some(42));
    assert!(!terrain.is_current(&snapshot.key));
    let next = Snapshot::capture(&terrain, [-1, -2]).unwrap();
    assert_eq!(next.block([-16, -64, -32]), Some(100));
    terrain.begin(Dimension {
        session: 7,
        epoch: 3,
        name: "example:replacement".into(),
        min_y: -64,
        height: 16,
        has_skylight: true,
    });
    assert!(!terrain.is_current(&next.key));
}

#[test]
fn unallocated_sky_above_resident_data_is_full_sky() {
    // Verified with the pinned client's SkyLightSectionStorage.getLightValue(false).
    let terrain = terrain();
    let snapshot = Snapshot::capture(&terrain, [-1, -2]).unwrap();
    assert_eq!(snapshot.light([-16, -48, -32]), Some([0, 15]));
}
