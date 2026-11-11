use super::*;
use assets::registry::java::physics::dynamic::{self, Kind};

#[test]
fn offset_collision_and_missing_entity_defaults_match_native_runtime_vectors() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-dynamic-collision.json"
    ))
    .unwrap();
    for row in fixture["coordinate"].as_array().unwrap() {
        let id = row["state"].as_u64().unwrap() as u32;
        let position = std::array::from_fn(|i| row["position"][i].as_i64().unwrap() as i32);
        let shape = match dynamic::kind(registry, id).unwrap() {
            Kind::Offset => dynamic::offset_shape(registry, id, position).unwrap(),
            Kind::Shulker | Kind::Moving => dynamic::missing_block_entity(registry, id)
                .unwrap()
                .to_vec(),
            Kind::PowderSnow => unreachable!(),
        };
        let expected = row["boxes"].as_array().unwrap();
        assert_eq!(shape.len(), expected.len(), "{row}");
        for (actual, expected) in shape.iter().zip(expected) {
            let vector = |start| {
                std::array::from_fn::<_, 3, _>(|i| expected[start + i].as_f64().unwrap().to_bits())
            };
            assert_eq!(actual.min.map(f64::to_bits), vector(0), "{row}");
            assert_eq!(actual.max.map(f64::to_bits), vector(3), "{row}");
        }
    }
}

#[test]
fn powder_collision_matches_native_falling_equipment_tag_and_descending_contexts() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let id = climbing::id(registry, "minecraft:powder_snow", &[]);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/java-dynamic-collision.json"
    ))
    .unwrap();
    for row in fixture["powder"].as_array().unwrap() {
        let shape = dynamic::powder_shape(
            registry,
            id,
            [0; 3],
            row["bottom"].as_f64().unwrap(),
            row["descending"].as_bool().unwrap(),
            row["fall"].as_f64().unwrap(),
            row["boots"].as_bool().unwrap() || row["walkable"].as_bool().unwrap(),
        )
        .unwrap();
        let expected = row["boxes"].as_array().unwrap();
        assert_eq!(shape.len(), expected.len(), "{row}");
        for (actual, expected) in shape.iter().zip(expected) {
            assert_eq!(
                actual.min.map(f64::to_bits),
                std::array::from_fn(|i| expected[i].as_f64().unwrap().to_bits()),
                "{row}"
            );
            assert_eq!(
                actual.max.map(f64::to_bits),
                std::array::from_fn(|i| expected[i + 3].as_f64().unwrap().to_bits()),
                "{row}"
            );
        }
    }
}

#[test]
fn restored_dynamic_shapes_distinguish_offsets_missing_entities_and_open_lids() {
    let Some(physics) = current_reference::native_physics() else {
        return;
    };
    let registry = physics.registry().unwrap();
    let mut terrain = climbing::scene([0; 3]);
    let shulker = climbing::id(registry, "minecraft:shulker_box", &[("facing", "up")]);
    let moving = climbing::id(
        registry,
        "minecraft:moving_piston",
        &[("facing", "up"), ("type", "normal")],
    );
    let shape = Body::default().bounds([8.5, 0.2, 8.5], 1.);
    let context = collision::PlayerContext {
        bottom: 0.2,
        descending: false,
    };
    assert!(
        collision::restored_block_collides(&terrain, &physics, [8, 0, 8], shulker, shape, context)
            .unwrap()
    );
    assert!(
        !collision::restored_block_collides(&terrain, &physics, [8, 0, 8], moving, shape, context)
            .unwrap()
    );
    terrain.set_block([8, 0, 8], world::java::BlockStateId(shulker));
    terrain.set_block_event([8, 0, 8], registry.block_id(shulker).unwrap(), 1, 1);
    assert!(matches!(
        collision::restored_block_collides(&terrain, &physics, [8, 0, 8], shulker, shape, context),
        Err(collision::QueryError::Dynamic { .. })
    ));
    let mut lid = world::java::shulker::Motion::default();
    lid.event(1);
    for _ in 0..5 {
        lid.tick();
    }
    terrain.shulkers.insert(
        [8, 0, 8],
        world::java::shulker::State {
            generation: terrain.block_generation([8, 0, 8]).unwrap(),
            block_id: registry.block_id(shulker).unwrap(),
            states: dynamic::shulker_states(registry.block_id(shulker).unwrap()).unwrap(),
            motion: lid,
        },
    );
    let above = Body::default().bounds([8.5, 1.1, 8.5], 1.);
    assert!(
        collision::restored_block_collides(&terrain, &physics, [8, 0, 8], shulker, above, context)
            .unwrap(),
        "canonical logical progress expands the active lid collision"
    );
    let powder = climbing::id(registry, "minecraft:powder_snow", &[]);
    assert!(
        collision::restored_living_block_collides(
            &terrain,
            &physics,
            [8, 0, 8],
            powder,
            shape,
            context,
            (3., false)
        )
        .unwrap()
    );
    assert!(
        !collision::restored_living_block_collides(
            &terrain,
            &physics,
            [8, 0, 8],
            powder,
            shape,
            context,
            (2.5, false)
        )
        .unwrap()
    );
}
