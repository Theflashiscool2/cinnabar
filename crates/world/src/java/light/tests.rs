use super::*;
use crate::java::{BiomeId, BlockStateId, Column, Palette, Section};

fn dimension() -> Dimension {
    Dimension {
        session: 7,
        epoch: 1,
        name: "fixture:sky".into(),
        min_y: -16,
        height: 48,
        has_skylight: true,
    }
}

fn column() -> Column {
    let mut sky = vec![0; 2048];
    // Local (3,0,2)=7 and (3,1,2)=11; the other nibbles remain zero.
    sky[17] = 0x70;
    sky[145] = 0xb0;
    let section = Arc::new(Section {
        blocks: Palette::uniform(BlockStateId(0), 4096),
        biomes: Palette::uniform(BiomeId(0), 64),
    });
    Column::new(
        vec![section; 3],
        Light {
            sky: vec![None, Some(Arc::from([])), None, Some(sky.into()), None],
            block: vec![None, None, None, Some(vec![0x99; 2048].into()), None],
        },
        vec![],
        vec![],
    )
}

#[test]
fn native_light_storage_vectors_include_gaps_and_negative_coordinates() {
    // Independent values from the pinned client's original light-storage classes.
    // The probe installed sky section -1=empty and section 1 with two distinct rows,
    // and block section 1=9, then called the visible-data lookup at each coordinate.
    let mut terrain = Terrain::default();
    terrain.begin(dimension());
    assert!(terrain.replace([-1, -2], column()));
    for (y, expected) in [
        (-32, [0, 0]),
        (-16, [0, 0]),
        (0, [0, 7]),
        (15, [0, 7]),
        (16, [9, 7]),
        (17, [9, 11]),
        (32, [0, 15]),
        (400, [0, 15]),
    ] {
        assert_eq!(terrain.light([-13, y, -30]), Some(expected), "y={y}");
    }
    assert_eq!(terrain.light([-14, 0, -30]), Some([0, 0]));
    assert_eq!(terrain.light([3, 17, 2]), None); // Unreceived column, not native empty storage.
}

#[test]
fn no_sky_dimension_and_replacement_retire_previous_light() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension());
    terrain.replace([-1, -2], column());
    let old = terrain.publication_key([-1, -2]).unwrap();
    let mut dark = dimension();
    dark.epoch += 1;
    dark.has_skylight = false;
    terrain.begin(dark);
    assert_eq!(terrain.light([-13, 17, -30]), None);
    terrain.replace([-1, -2], column());
    assert!(!terrain.is_current(&old));
    assert_eq!(terrain.light([-13, 17, -30]), Some([9, 0]));
    assert_eq!(terrain.light([-13, 400, -30]), Some([0, 0]));
    terrain.unload([-1, -2]);
    assert_eq!(terrain.light([-13, 17, -30]), None);
}

#[test]
fn light_updates_preserve_omitted_layers_and_invalidate_snapshots() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension());
    terrain.replace([-1, -2], column());
    let old = terrain.publication_key([-1, -2]).unwrap();
    let snapshot = Arc::clone(terrain.column([-1, -2]).unwrap());
    assert!(terrain.update_light(
        [-1, -2],
        Light {
            sky: vec![None, None, Some(Arc::from([])), None, None],
            block: vec![None; 5],
        }
    ));
    assert!(!terrain.is_current(&old));
    assert_eq!(
        snapshot.light.sample(&dimension(), [-13, 0, -30]),
        Some([0, 7])
    );
    assert_eq!(terrain.light([-13, 0, -30]), Some([0, 0]));
    assert_eq!(terrain.light([-13, 17, -30]), Some([9, 11]));
}

#[test]
fn malformed_arrays_cannot_replace_or_mutate_authority() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension());
    terrain.replace([-1, -2], column());
    let old = terrain.publication_key([-1, -2]).unwrap();
    let mut malformed = column();
    malformed.light.sky[3] = Some(vec![1; 2047].into());
    assert_eq!(malformed.light.sample(&dimension(), [-13, 17, -30]), None);
    assert!(!terrain.update_light([-1, -2], malformed.light.clone()));
    assert!(!terrain.replace([-1, -2], malformed));
    assert!(terrain.is_current(&old));
    assert_eq!(terrain.light([-13, 17, -30]), Some([9, 11]));
}

#[test]
fn dirty_light_regions_follow_native_sky_inheritance_and_ignore_unchanged_rows() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension());
    terrain.replace([-1, -2], column());
    terrain.take_dirty();
    let mut upper = terrain.column([-1, -2]).unwrap().light.sky[3]
        .as_ref()
        .unwrap()
        .to_vec();
    upper[145] = 0x20; // Change y=17, leaving the inherited bottom row unchanged.
    let update = |bytes: Vec<u8>| Light {
        sky: vec![None, None, None, Some(bytes.into()), None],
        block: vec![None; 5],
    };
    assert!(terrain.update_light([-1, -2], update(upper.clone())));
    assert!(terrain.take_dirty().is_empty());
    let changed: Vec<_> = terrain.take_changed_regions().collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].min, [-16, 16, -32]);
    assert_eq!(changed[0].max, [-1, 31, -17]);
    assert_eq!(terrain.light([-13, 0, -30]), Some([0, 7]));
    assert_eq!(terrain.light([-13, 17, -30]), Some([9, 2]));
    upper[17] = 0x30; // Changing the bottom row also changes the unallocated layer below.
    assert!(terrain.update_light([-1, -2], update(upper)));
    let changed: Vec<_> = terrain.take_changed_regions().map(|r| r.min[1]).collect();
    assert_eq!(changed, [0, 16]);
    assert_eq!(terrain.light([-13, 0, -30]), Some([0, 3]));
    assert_eq!(terrain.light([-13, -16, -30]), Some([0, 0]));
}

#[test]
fn repeated_or_omitted_light_data_keeps_existing_worker_authority() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension());
    terrain.replace([-1, -2], column());
    terrain.take_dirty();
    let key = terrain.publication_key([-1, -2]).unwrap();
    let same = terrain.column([-1, -2]).unwrap().light.clone();
    for update in [
        same,
        Light {
            sky: vec![None; 5],
            block: vec![None; 5],
        },
    ] {
        assert!(terrain.update_light([-1, -2], update));
        assert!(terrain.is_current(&key));
        assert!(terrain.take_dirty().is_empty());
        assert!(terrain.take_changed_regions().next().is_none());
    }
}
