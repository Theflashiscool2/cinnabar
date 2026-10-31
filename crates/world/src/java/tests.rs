use super::*;
fn dimension(epoch: u64) -> Dimension {
    Dimension {
        session: 7,
        epoch,
        name: "minecraft:overworld".into(),
        min_y: -64,
        height: 16,
        has_skylight: true,
    }
}
fn column() -> Column {
    Column::new(
        vec![Arc::new(Section {
            blocks: Palette::uniform(BlockStateId(7), 4096),
            biomes: Palette::uniform(BiomeId(0), 64),
        })],
        Light {
            sky: vec![None; 3],
            block: vec![None; 3],
        },
        vec![],
        vec![],
    )
}
#[test]
fn negatives_updates_and_missing_data_are_distinct() {
    let mut t = Terrain::default();
    t.begin(dimension(1));
    t.replace([-1, -2], column());
    assert_eq!(t.block([-1, -64, -17]), Some(BlockStateId(7)));
    assert_eq!(t.block([-1, -65, -17]), None);
    assert_eq!(t.block([0, -64, 0]), None);
    t.set_block([-1, -64, -17], BlockStateId(982_000)); // Unknown IDs survive; never air fallback.
    assert_eq!(t.block([-1, -64, -17]), Some(BlockStateId(982_000)));
    assert_eq!(t.block([-2, -64, -17]), Some(BlockStateId(7)));
    t.set_block([0, -64, 0], BlockStateId(1));
    assert_eq!(t.len(), 1);
    assert_eq!(t.ignored_nonresident_updates, 1);
}
#[test]
fn worker_results_cannot_cross_updates_reloads_or_dimensions() {
    let mut t = Terrain::default();
    t.begin(dimension(1));
    t.replace([0, 0], column());
    let original = t.publication_key([0, 0]).unwrap();
    let snapshot = Arc::clone(t.column([0, 0]).unwrap());
    t.set_block([0, -64, 0], BlockStateId(8));
    assert!(!t.is_current(&original));
    assert_eq!(snapshot.sections[0].blocks.get(0), Some(BlockStateId(7)));
    let changed = t.publication_key([0, 0]).unwrap();
    t.unload([0, 0]);
    t.replace([0, 0], column());
    assert!(!t.is_current(&changed));
    let reloaded = t.publication_key([0, 0]).unwrap();
    t.begin(dimension(2));
    t.replace([0, 0], column());
    assert!(!t.is_current(&reloaded));
    assert!(!t.accepts(7, 1));
    assert!(!t.accepts(6, 2));
}
#[test]
fn omitted_light_keeps_storage_and_empty_light_clears_previous_data() {
    let mut t = Terrain::default();
    t.begin(dimension(1));
    t.replace([0, 0], column());
    t.update_light(
        [0, 0],
        Light {
            sky: vec![None, Some(Arc::from(vec![255; 2048])), None],
            block: vec![None; 3],
        },
    );
    t.update_light(
        [0, 0],
        Light {
            sky: vec![None, Some(Arc::from([])), None],
            block: vec![None; 3],
        },
    );
    let c = t.column([0, 0]).unwrap();
    assert!(c.light.sky[0].is_none());
    assert_eq!(c.light.sky[1].as_deref(), Some(&[][..]));
}
#[test]
fn edits_preserve_yzx_coordinates_and_biomes() {
    let mut t = Terrain::default();
    t.begin(dimension(1));
    t.replace([0, 0], column());
    t.set_block([1, -62, 3], BlockStateId(9));
    let s = &t.column([0, 0]).unwrap().sections[0];
    assert_eq!(s.blocks.get(2 * 256 + 3 * 16 + 1), Some(BlockStateId(9)));
    t.update_biomes([0, 0], vec![Palette::uniform(BiomeId(3), 64)]);
    assert_eq!(t.block([1, -62, 3]), Some(BlockStateId(9)));
    assert_eq!(
        t.column([0, 0]).unwrap().sections[0].biomes.get(0),
        Some(BiomeId(3))
    );
}

#[test]
fn neighbour_replacement_invalidates_lighting_and_mesh_snapshot() {
    let mut t = Terrain::default();
    t.begin(dimension(1));
    t.replace([0, 0], column());
    let key = t.publication_key([0, 0]).unwrap();
    t.replace([1, 1], column());
    assert!(!t.is_current(&key));
    assert!(t.take_dirty().contains(&[0, 0]));
}

#[test]
fn spatial_edits_coalesce_within_sections_without_dirtying_whole_columns() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension(1));
    terrain.replace([-1, 0], column());
    terrain.take_dirty();
    terrain.set_block([-15, -63, 2], BlockStateId(8));
    terrain.set_block([-2, -50, 12], BlockStateId(9));
    assert!(terrain.take_dirty().is_empty());
    assert_eq!(
        terrain.take_changed_regions().collect::<Vec<_>>(),
        [ChangedRegion {
            min: [-15, -63, 2],
            max: [-2, -50, 12],
        }]
    );
    let key = terrain.publication_key([-1, 0]).unwrap();
    terrain.set_block([-2, -50, 12], BlockStateId(9));
    assert!(terrain.is_current(&key));
    assert!(terrain.take_changed_regions().next().is_none());
    terrain.set_block([-2, -50, 12], BlockStateId(10));
    terrain.unload([-1, 0]);
    assert!(terrain.take_changed_regions().next().is_none());
    terrain.replace([-1, 0], column());
    assert!(!terrain.is_current(&key));
}

#[test]
fn block_entity_events_are_resident_replacement_and_session_scoped() {
    let mut t = Terrain::default();
    t.begin(dimension(1));
    let pos = [-1, -64, -17];
    assert!(!t.set_block_event(pos, 3, 1, 1));
    t.replace([-1, -2], column());
    assert!(t.set_block_event(pos, 3, 1, 2));
    let first = t.block_event(pos).unwrap();
    assert_eq!(first.binary_transition, None);
    t.set_block_event(pos, 3, 1, 1);
    t.set_block_event(pos, 3, 1, 3);
    assert_eq!(t.block_event(pos).unwrap().binary_transition, Some(1));
    t.set_block_event(pos, 3, 1, 0);
    assert!(t.block_event(pos).unwrap().revision > first.revision);
    assert_eq!(t.block_event(pos).unwrap().value, 0);
    assert_eq!(t.block_event(pos).unwrap().binary_transition, Some(0));
    t.set_block(pos, BlockStateId(8));
    assert!(t.block_event(pos).is_none());
    let entity = BlockEntity {
        position: pos,
        kind: 5,
        data: None,
    };
    assert!(t.set_block_entity(entity.clone()));
    assert!(t.set_block_entity(entity));
    assert_eq!(t.column([-1, -2]).unwrap().block_entities.len(), 1);
    t.set_block_event(pos, 3, 1, 1);
    t.replace([-1, -2], column());
    assert!(t.block_event(pos).is_none());
    assert!(t.column([-1, -2]).unwrap().block_entities.is_empty());
    t.set_block_event(pos, 3, 1, 1);
    t.unload([-1, -2]);
    assert!(t.block_event(pos).is_none());
    assert!(!t.set_block_event(pos, 3, 1, 1));
    t.replace([-1, -2], column());
    t.set_block_event(pos, 3, 1, 1);
    t.begin(dimension(2));
    assert!(t.block_event(pos).is_none());
    assert!(!t.accepts(7, 1));
}

#[test]
fn chest_property_changes_keep_authority_but_reused_positions_get_new_lifetimes() {
    let mut terrain = Terrain::default();
    terrain.begin(dimension(1));
    terrain.replace([0, 0], column());
    terrain.install_block_types(Arc::from([0, 1, 2, 3, 4, 5, 6, 42, 42, 43]));
    let pos = [0, -64, 0];
    let birth = terrain.block_generation(pos).unwrap();
    terrain.set_block_entity(BlockEntity {
        position: pos,
        kind: 1,
        data: None,
    });
    terrain.set_block_event(pos, 42, 1, 1);
    terrain.set_block(pos, BlockStateId(8)); // same block, another facing/half/waterlogging state.
    assert_eq!(terrain.block_generation(pos), Some(birth));
    assert_eq!(terrain.block_event(pos).unwrap().value, 1);
    assert_eq!(terrain.column([0, 0]).unwrap().block_entities.len(), 1);
    terrain.set_block(pos, BlockStateId(9)); // different block, not merely a different model.
    assert!(terrain.block_event(pos).is_none());
    assert!(terrain.column([0, 0]).unwrap().block_entities.is_empty());
    terrain.set_block(pos, BlockStateId(7));
    assert_ne!(terrain.block_generation(pos), Some(birth));
    assert!(terrain.block_event(pos).is_none());
    let reused = terrain.block_generation(pos);
    terrain.replace([0, 0], column());
    assert_ne!(terrain.block_generation(pos), reused);
}
