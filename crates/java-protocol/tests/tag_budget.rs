use java_protocol::session::facts::Configuration;

#[test]
fn tag_budget_counts_retained_registries_and_rejects_atomically() {
    let mut state = Configuration::default();
    // Capacity is retained memory even when a decoded list is later cleared.
    let large = || {
        [(
            "fixture:large".into(),
            Vec::<i32>::with_capacity(9 * 1024 * 1024),
        )]
        .into()
    };
    state
        .update_tags([("minecraft:block".into(), large())].into())
        .unwrap();
    assert!(
        state
            .update_tags([("minecraft:item".into(), large())].into())
            .is_err()
    );
    assert_eq!(state.tags.len(), 1);
    assert!(state.tags["minecraft:block"].contains_key("fixture:large"));
    // Replacing an existing registry releases its old allocation, so it is not double counted.
    state
        .update_tags([("minecraft:block".into(), large())].into())
        .unwrap();
    state
        .update_tags([("minecraft:block".into(), Default::default())].into())
        .unwrap();
    state
        .update_tags([("minecraft:item".into(), large())].into())
        .unwrap();
    assert!(state.tags["minecraft:block"].is_empty());
}
