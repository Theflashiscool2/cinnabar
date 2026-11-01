use super::*;

#[test]
fn dimension_skylight_follows_native_registry_on_replacement() {
    let mut state = super::super::tests_support::state();
    let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    begin(&mut state, &spawn).unwrap();
    assert!(state.terrain.dimension().unwrap().has_skylight);
    let epoch = state.dimension_epoch;
    set_flag(&mut state, Some(Value::Byte(0)));
    begin(&mut state, &spawn).unwrap();
    assert!(!state.terrain.dimension().unwrap().has_skylight);
    assert_eq!(state.dimension_epoch, epoch + 1);
}

#[test]
fn malformed_skylight_flag_does_not_replace_current_dimension() {
    for flag in [
        None,
        Some(Value::Int(1)),
        Some(Value::String("true".into())),
    ] {
        let mut state = super::super::tests_support::state();
        let spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
        let dimension = state.terrain.dimension().unwrap().clone();
        set_flag(&mut state, flag);
        assert_eq!(
            begin(&mut state, &spawn),
            Err("missing Java dimension skylight flag")
        );
        assert_eq!(state.terrain.dimension(), Some(&dimension));
        assert_eq!(state.dimension_epoch, dimension.epoch);
    }
}

fn set_flag(state: &mut WorldState, flag: Option<Value>) {
    let entries = state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .registries
        .get_mut("minecraft:dimension_type")
        .unwrap();
    let Value::Compound(fields) = &mut entries[0].value else {
        panic!("fixture compound")
    };
    if let Some(flag) = flag {
        fields.insert("has_skylight".into(), flag);
    } else {
        fields.remove("has_skylight");
    }
}

#[test]
fn spawn_seed_is_retained_and_replaced_without_rehashing() {
    let mut state = super::super::tests_support::state();
    let mut spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
    spawn.hashed_seed = i64::MIN + 37;
    begin(&mut state, &spawn).unwrap();
    assert_eq!(state.terrain.biome_seed(), Some(spawn.hashed_seed));
    spawn.hashed_seed = 91234;
    begin(&mut state, &spawn).unwrap();
    assert_eq!(state.terrain.biome_seed(), Some(91234));
}
