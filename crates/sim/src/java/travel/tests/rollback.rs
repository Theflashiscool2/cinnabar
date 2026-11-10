use super::*;

#[test]
fn restored_block_intersection_matches_native_entity_shape_and_context() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-block-rollback.json"
    ))
    .unwrap();
    let vector = |v: &serde_json::Value| std::array::from_fn::<_, 3, _>(|i| v[i].as_f64().unwrap());
    for case in cases["cases"].as_array().unwrap() {
        let cell = [case["cell"].as_i64().unwrap() as i32; 3];
        let terrain = climbing::scene(cell);
        let region = Bounds {
            min: vector(&case["min"]),
            max: vector(&case["max"]),
        };
        let actual = collision::restored_block_collides(
            &terrain,
            &physics,
            cell,
            case["state"].as_u64().unwrap() as u32,
            region,
            collision::PlayerContext {
                bottom: region.min[1],
                descending: case["descending"].as_bool().unwrap(),
            },
        )
        .unwrap();
        assert_eq!(actual, case["collides"].as_bool().unwrap(), "{case}");
    }
}

#[test]
fn restored_block_query_uses_supplied_state_without_unrelated_terrain_queries() {
    let (terrain, physics) = terrain::scene();
    let context = collision::PlayerContext {
        bottom: 0.,
        descending: false,
    };
    let body = Bounds {
        min: [100.2, 0.1, 100.2],
        max: [100.8, 0.4, 100.8],
    };
    // The palette cell is outside loaded chunks: static restored-state collision is local.
    assert!(
        collision::restored_block_collides(&terrain, &physics, [100, 0, 100], 1, body, context)
            .unwrap()
    );
    assert!(
        !collision::restored_block_collides(&terrain, &physics, [100, 0, 100], 3, body, context)
            .unwrap()
    );
    assert!(
        !collision::restored_block_collides(&terrain, &physics, [100, 0, 100], 0, body, context)
            .unwrap()
    );
    assert!(matches!(
        collision::restored_block_collides(&terrain, &physics, [100, 0, 100], 10, body, context),
        Err(collision::QueryError::Unknown(10))
    ));
}
