use super::*;
const CUBE: Bounds = Bounds {
    min: [0.; 3],
    max: [1.; 3],
};

#[test]
fn dynamic_outline_is_picked_without_bypassing_missing_context_or_retired_worlds() {
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use world::java::{BiomeId, BlockStateId, Column, Dimension, Light, Palette, Section};
    let mut bytes = b"CBJHITS1".to_vec();
    bytes.extend(2u32.to_be_bytes());
    for id in 0..2u32 {
        bytes.extend(id.to_be_bytes());
        bytes.push(id as u8); // air is static/empty, state one needs context.
        bytes.extend(1f32.to_be_bytes());
        bytes.push(0);
        bytes.extend([0; 8]);
    }
    let table = Table::load(&bytes, &format!("{:x}", Sha256::digest(&bytes)), 2).unwrap();
    let mut terrain = Terrain::default();
    terrain.begin(Dimension {
        session: 1,
        epoch: 2,
        name: "test".into(),
        min_y: 0,
        height: 16,
        has_skylight: true,
    });
    assert!(terrain.replace(
        [-1, -1],
        Column::new(
            vec![Arc::new(Section {
                blocks: Palette::uniform(BlockStateId(0), 4096),
                biomes: Palette::uniform(BiomeId(0), 64),
            })],
            Light {
                sky: vec![None; 3],
                block: vec![None; 3]
            },
            vec![],
            vec![],
        ),
    ));
    terrain.set_block([-2, 1, -2], BlockStateId(1));
    let from = [-1.5, 1.8, -4.];
    let to = [-1.5, 1.8, -0.5];
    assert_eq!(
        pick(&terrain, &table, (1, 2), from, to),
        Err(Error::Contextual(1))
    );
    let hit = pick_with_context(&terrain, &table, (1, 2), from, to, |p, id| {
        assert_eq!(p, [-2, 1, -2]);
        assert_eq!(id, 1);
        Some(Bounds {
            min: [0., 0., -0.5],
            max: [1.; 3],
        })
    })
    .unwrap()
    .unwrap();
    assert_eq!(hit.block, [-2, 1, -2]);
    assert_eq!(hit.position, [-1.5, 1.8, -2.5]);
    assert_eq!(hit.face, Face::North);
    terrain.set_block([-2, 1, -2], BlockStateId(0));
    assert!(
        pick_with_context(&terrain, &table, (1, 2), from, to, |_, _| panic!(
            "removed host"
        ))
        .unwrap()
        .is_none()
    );
    assert_eq!(
        pick_with_context(&terrain, &table, (9, 2), from, to, |_, _| Some(CUBE)),
        Err(Error::Retired)
    );
}

#[test]
fn independent_pinned_native_shape_vectors() {
    let hit = clip(&[CUBE], [-2, -3, -4], [-1.5, -2.5, -7.], [-1.5, -2.5, -2.]).unwrap();
    assert_eq!(hit.position, [-1.5, -2.5, -4.]);
    assert_eq!(hit.face, Face::North);
    assert!(!hit.inside);
    let inside = clip(&[CUBE], [0; 3], [0.5; 3], [2.5; 3]).unwrap();
    assert_eq!(inside.position, [0.502; 3]);
    assert_eq!(inside.face, Face::Down);
    assert!(inside.inside);
    let edge = clip(&[CUBE], [0; 3], [-1., -1., 0.5], [2., 2., 0.5]).unwrap();
    assert_eq!(edge.position, [0., 0., 0.5]);
    assert_eq!(edge.face, Face::West);
    let slab = Bounds {
        min: [0.; 3],
        max: [1., 0.5, 1.],
    };
    assert!(clip(&[slab], [0; 3], [0.5, 0.75, -2.], [0.5, 0.75, 2.]).is_none());
    let top = clip(&[slab], [0; 3], [0.5, 2., 0.5], [0.5, -2., 0.5]).unwrap();
    assert_eq!(top.position, [0.5, 0.5, 0.5]);
    assert_eq!(top.face, Face::Up);
    assert!(clip(&[CUBE], [0; 3], [0.5, 0.5, -2.], [0.5, 0.5, 0.]).is_none());
}

#[test]
fn traversal_preserves_signed_cells_and_native_corner_tie_order() {
    let mut cells = Vec::new();
    let result = traverse([-0.5, -0.5, -0.5], [1.5, 1.5, 1.5], |p| {
        cells.push(p);
        Ok(None::<()>)
    })
    .unwrap();
    assert_eq!(result, None);
    assert_eq!(
        cells,
        vec![
            [-1, -1, -1],
            [-1, -1, 0],
            [-1, 0, 0],
            [0, 0, 0],
            [0, 0, 1],
            [0, 1, 1],
            [1, 1, 1]
        ]
    );
    let mut visited = 0;
    assert_eq!(
        traverse([0.5; 3], [2.5; 3], |_| {
            visited += 1;
            Err::<Option<()>, _>(Error::Missing([0; 3]))
        }),
        Err(Error::Missing([0; 3]))
    );
    assert_eq!(visited, 1);
    assert_eq!(
        traverse([f64::NAN, 0., 0.], [1.; 3], |_| Ok(None::<()>)),
        Err(Error::Ray)
    );
    assert_eq!(
        traverse([0.; 3], [0.; 3], |_| panic!("zero-length ray")),
        Ok(None::<()>)
    );
}
