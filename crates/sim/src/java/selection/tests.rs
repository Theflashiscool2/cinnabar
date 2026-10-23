use super::*;
const CUBE: Bounds = Bounds {
    min: [0.; 3],
    max: [1.; 3],
};

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
