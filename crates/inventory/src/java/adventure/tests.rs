use super::*;
use std::sync::Arc;
fn block(name: &str, properties: &[(&str, &str)]) -> BlockState {
    BlockState {
        name: Arc::from(name),
        properties: properties
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        is_default: true,
    }
}
fn target<'a>(block: &'a BlockState, tags: &'a BTreeMap<String, Vec<i32>>) -> Target<'a> {
    Target {
        state: 1,
        block,
        tags,
        tag_revision: 0,
    }
}
#[test]
fn block_ids_tags_or_and_native_component_matcher_exclusion_are_exact() {
    let facts = assets::registry::java::adventure::builtin().unwrap();
    let id = facts.block("minecraft:stone").unwrap().0;
    let stone = block("minecraft:stone", &[]);
    let tags = [("fixture:allowed".into(), vec![id as i32])].into();
    let context = target(&stone, &tags);
    assert!(evaluate(&serde_json::json!([{"blocks":{"ids":[id]},"properties":null,"nbt":null,"components":{"exact":{"minecraft:custom_name":"irrelevant"}}}]), &context).unwrap());
    assert!(
        evaluate(
            &serde_json::json!([{"blocks":{"ids":[]}}, {"blocks":{"tag":"fixture:allowed"}}]),
            &context
        )
        .unwrap()
    );
    assert!(
        !evaluate(
            &serde_json::json!([{"blocks":{"tag":"fixture:missing"}}]),
            &context
        )
        .unwrap()
    );
    assert!(!evaluate(&serde_json::json!([]), &context).unwrap());
}
#[test]
fn exact_and_ranged_properties_use_native_parse_domains_and_enum_order() {
    let empty = BTreeMap::new();
    let stairs = block(
        "minecraft:oak_stairs",
        &[
            ("facing", "north"),
            ("half", "bottom"),
            ("shape", "straight"),
            ("waterlogged", "false"),
        ],
    );
    assert!(
        evaluate(
            &serde_json::json!([{"properties":[{"name":"facing","min":"north","max":"east"}]}]),
            &target(&stairs, &empty)
        )
        .unwrap()
    );
    assert!(
        !evaluate(
            &serde_json::json!([{"properties":[{"name":"missing","value":"false"}]}]),
            &target(&stairs, &empty)
        )
        .unwrap()
    );
    let wheat = block("minecraft:wheat", &[("age", "1")]);
    for value in ["1", "+01", "١"] {
        assert!(
            evaluate(
                &serde_json::json!([{"properties":[{"name":"age","value":value}]}]),
                &target(&wheat, &empty)
            )
            .unwrap()
        );
    }
    assert!(
        !evaluate(
            &serde_json::json!([{"properties":[{"name":"age","min":"-1","max":"7"}]}]),
            &target(&wheat, &empty)
        )
        .unwrap(),
        "invalid bound must fail even when numeric comparison would pass"
    );
    assert!(
        evaluate(
            &serde_json::json!([{"properties":[{"name":"age","min":null,"max":null}]}]),
            &target(&wheat, &empty)
        )
        .unwrap()
    );
}
#[test]
fn nbt_requires_real_block_entity_serialization_never_latest_packet_nbt() {
    let empty = BTreeMap::new();
    let stone = block("minecraft:stone", &[]);
    let chest = block("minecraft:chest", &[]);
    let predicate = serde_json::json!([{"nbt":{"Compound":{}}}]);
    assert_eq!(evaluate(&predicate, &target(&stone, &empty)), Ok(false));
    assert!(
        evaluate(&predicate, &target(&chest, &empty))
            .unwrap_err()
            .contains("saveWithFullMetadata")
    );
}
fn permission_stack(value: Value) -> Stack {
    Stack {
        item: "minecraft:iron_pickaxe".into(),
        count: 2,
        added: [
            (Permission::Break.component().into(), value.clone()),
            (Permission::PlaceOn.component().into(), value),
        ]
        .into(),
        removed: BTreeSet::new(),
    }
}
#[test]
fn native_same_state_cache_survives_tag_rebind_and_count_but_authority_replaces_component() {
    let facts = assets::registry::java::adventure::builtin().unwrap();
    let id = facts.block("minecraft:stone").unwrap().0;
    let stone = block("minecraft:stone", &[]);
    let allowed = [("fixture:allowed".into(), vec![id as i32])].into();
    let empty = BTreeMap::new();
    let mut state = State::default();
    state.begin(7, 1);
    let mut stack = permission_stack(serde_json::json!([{"blocks":{"tag":"fixture:allowed"}}]));
    stack.added.insert(
        Permission::PlaceOn.component().into(),
        serde_json::json!([]),
    );
    state
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(stack.clone()),
            },
        )
        .unwrap();
    assert!(
        state
            .test_adventure(
                BlockUseHand::Main,
                Permission::Break,
                target(&stone, &allowed)
            )
            .unwrap()
    );
    assert!(
        !state
            .test_adventure(
                BlockUseHand::Main,
                Permission::PlaceOn,
                target(&stone, &allowed)
            )
            .unwrap(),
        "can_place_on does not reuse can_break's true cache"
    );
    assert!(state.predict_block_use(7, 1, 1, BlockUseHand::Main, &stack));
    let rebound = Target {
        tag_revision: 1,
        ..target(&stone, &empty)
    };
    assert!(
        state
            .test_adventure(BlockUseHand::Main, Permission::Break, rebound)
            .unwrap(),
        "native cache does not recheck changed tags for identical state"
    );
    state
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(stack.clone()),
            },
        )
        .unwrap();
    assert!(
        !state
            .test_adventure(
                BlockUseHand::Main,
                Permission::Break,
                Target {
                    tag_revision: 1,
                    ..target(&stone, &empty)
                }
            )
            .unwrap(),
        "new wire stack has a new predicate object/cache"
    );
    assert!(
        !state
            .test_adventure(
                BlockUseHand::Main,
                Permission::PlaceOn,
                target(&stone, &empty)
            )
            .unwrap(),
        "can_place_on has a separate component cache"
    );
    state.rebind_player(7, 2);
    assert!(
        !state
            .test_adventure(
                BlockUseHand::Main,
                Permission::Break,
                target(&stone, &allowed)
            )
            .unwrap(),
        "retained player keeps native predicate objects"
    );
}

#[test]
fn copied_tag_predicate_after_rebinding_is_explicit_until_server_replaces_the_stack() {
    let facts = assets::registry::java::adventure::builtin().unwrap();
    let id = facts.block("minecraft:stone").unwrap().0;
    let stone = block("minecraft:stone", &[]);
    let tags = [("fixture:allowed".into(), vec![id as i32])].into();
    let mut state = State::default();
    state.begin(7, 1);
    let stack = permission_stack(serde_json::json!([{"blocks":{"tag":"fixture:allowed"}}]));
    let mut slots = vec![None; PLAYER_MENU_SIZE];
    slots[HOTBAR_START] = Some(stack.clone());
    state
        .admit(
            7,
            1,
            Update::Content {
                container: 0,
                state: 1,
                slots,
                carried: None,
            },
        )
        .unwrap();
    assert!(
        state
            .test_adventure(BlockUseHand::Main, Permission::Break, target(&stone, &tags))
            .unwrap()
    );
    let plan = state
        .plan_pickup(
            state.active_identity().unwrap(),
            HOTBAR_START,
            pickup::Button::Right,
            |_| Ok(64),
        )
        .unwrap();
    assert!(state.pickup_queued(plan, 1));
    assert!(
        state
            .test_adventure(
                BlockUseHand::Main,
                Permission::Break,
                Target {
                    tag_revision: 1,
                    ..target(&stone, &tags)
                }
            )
            .unwrap_err()
            .contains("cache identity")
    );
    state
        .admit(
            7,
            1,
            Update::PlayerSlot {
                slot: 0,
                stack: Some(stack),
            },
        )
        .unwrap();
    assert!(
        state
            .test_adventure(
                BlockUseHand::Main,
                Permission::Break,
                Target {
                    tag_revision: 1,
                    ..target(&stone, &tags)
                }
            )
            .unwrap()
    );
}
