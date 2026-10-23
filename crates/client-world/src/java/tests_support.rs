//! Shared native admission fixture with a real scoped dimension registry.
use super::WorldState;
use java_protocol::session::facts;

pub(super) fn state() -> WorldState {
    let mut state = WorldState::default();
    state.begin(7);
    state.dimension_epoch = 1;
    let spawn = facts::Spawn {
        dimension_type: 0,
        world: "minecraft:overworld".into(),
        hashed_seed: 0,
        game_mode: 0,
        previous_game_mode: -1,
        debug: false,
        flat: true,
        last_death: None,
        portal_cooldown: 0,
        sea_level: 63,
    };
    state.bootstrap = Some(Box::new(facts::Bootstrap {
        generation: 7,
        profile: java_protocol::auth::Profile {
            id: Default::default(),
            name: "Fixture".into(),
        },
        server_session: Default::default(),
        configuration: Default::default(),
        world: facts::World {
            entity_id: 42,
            hardcore: false,
            worlds: vec![spawn.world.clone()],
            max_players: 2,
            view_distance: 2,
            simulation_distance: 2,
            reduced_debug: false,
            respawn_screen: true,
            limited_crafting: false,
            spawn,
            online_mode: false,
            secure_chat: false,
        },
    }));
    state.terrain.begin(world::java::Dimension {
        session: 7,
        epoch: 1,
        name: "minecraft:overworld".into(),
        min_y: -64,
        height: 384,
        has_skylight: true,
    });
    state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .registries
        .insert(
            "minecraft:dimension_type".into(),
            vec![facts::RegistryEntry {
                name: "minecraft:overworld".into(),
                value: java_protocol::nbt::Value::Compound(
                    [
                        ("min_y".into(), java_protocol::nbt::Value::Int(-64)),
                        ("height".into(), java_protocol::nbt::Value::Int(384)),
                        ("has_skylight".into(), java_protocol::nbt::Value::Byte(1)),
                    ]
                    .into(),
                ),
            }],
        );
    let pig = |name: &str, model: Option<&str>| {
        use java_protocol::nbt::Value;
        let mut fields = std::collections::BTreeMap::from([
            (
                "asset_id".into(),
                Value::String(format!("minecraft:entity/pig/pig_{name}")),
            ),
            (
                "baby_asset_id".into(),
                Value::String(format!("minecraft:entity/pig/pig_{name}_baby")),
            ),
        ]);
        if let Some(model) = model {
            fields.insert("model".into(), Value::String(model.into()));
        }
        facts::RegistryEntry {
            name: format!("minecraft:{name}"),
            value: Value::Compound(fields),
        }
    };
    state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .registries
        .insert(
            "minecraft:pig_variant".into(),
            vec![
                pig("cold", Some("cold")),
                pig("temperate", None),
                pig("warm", None),
            ],
        );
    state.entities.begin(7, 1);
    state
}
