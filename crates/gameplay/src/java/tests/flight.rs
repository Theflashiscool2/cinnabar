use super::*;
use java_protocol::session::facts;
use std::sync::Arc;
use world::java::{BiomeId, BlockStateId, Column, Light, Palette, Section};

pub(super) fn flight_world() -> Option<(WorldState, Table)> {
    let physics = rollback_physics()?;
    let mut state = world(1);
    let spawn = facts::Spawn {
        dimension_type: 0,
        world: "fixture:movement".into(),
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
    state
        .bootstrap
        .as_mut()
        .unwrap()
        .configuration
        .registries
        .insert(
            "minecraft:dimension_type".into(),
            vec![facts::RegistryEntry {
                name: "fixture:movement".into(),
                value: java_protocol::nbt::Value::Compound(Default::default()),
            }],
        );
    state.player_dynamics.attributes = java_protocol::target::PLAYER_ATTRIBUTES
        .iter()
        .map(|&(id, base)| {
            let spec = java_protocol::target::ATTRIBUTES[id];
            (
                spec.name.to_owned(),
                Attribute {
                    base,
                    minimum: spec.minimum,
                    maximum: spec.maximum,
                    modifiers: vec![],
                },
            )
        })
        .collect();
    state.player_dynamics.abilities = Some(world::java::dynamics::Abilities {
        invulnerable: false,
        flying: true,
        may_fly: true,
        creative: false,
        flying_speed: 0.05,
        walking_speed: 0.1,
    });
    state.status.mode = Some(world::java::hud::GameMode::Survival);
    state.terrain.replace(
        [0, 0],
        Column::new(
            (0..24)
                .map(|_| {
                    Arc::new(Section {
                        blocks: Palette::uniform(BlockStateId(0), 4096),
                        biomes: Palette::uniform(BiomeId(0), 64),
                    })
                })
                .collect(),
            Light {
                sky: vec![None; 26],
                block: vec![None; 26],
            },
            vec![],
            vec![],
        ),
    );
    let stone = physics
        .registry()
        .unwrap()
        .state_id("minecraft:stone", &Default::default())
        .unwrap();
    state.terrain.set_block([8, 0, 8], BlockStateId(stone));
    state.player_motion = Some(Motion {
        position: [8.5, 1.1, 8.5],
        velocity: [0., -0.2, 0.],
        ..Default::default()
    });
    Some((state, physics))
}

#[test]
fn real_landing_disables_flight_without_changing_mayfly_and_held_jump_can_repeat() {
    let Some((mut state, physics)) = flight_world() else {
        return;
    };
    let mut controller = Controller::default();
    controller.begin_frame(&state, TICK);
    assert!(
        controller.tick(&mut state, &physics, Controls::default()),
        "{:?}",
        controller.blocked
    );
    assert!(controller.body.on_ground);
    let ability = state.player_dynamics.abilities.as_ref().unwrap();
    assert!(!ability.flying);
    assert!(ability.may_fly);
    assert_eq!(controller.pending_ability(), Some((1, 0, false)));
    assert_eq!(state.player_motion.unwrap().velocity[1], -0.12);
    controller.ability_queued();
    controller.queued();
    controller.begin_frame(&state, TICK);
    assert!(
        controller.tick(
            &mut state,
            &physics,
            Controls {
                travel: Input {
                    jump: true,
                    ..Default::default()
                },
                ..Default::default()
            }
        ),
        "{:?}",
        controller.blocked
    );
    assert!(state.player_motion.unwrap().position[1] > 1.);
    assert!(controller.pending_ability().is_none());
}

#[test]
fn spectator_uses_noclip_keeps_flight_and_auto_enables_only_with_server_permission() {
    let Some((mut state, physics)) = flight_world() else {
        return;
    };
    state.status.mode = Some(world::java::hud::GameMode::Spectator);
    state.player_dynamics.abilities.as_mut().unwrap().flying = false;
    let mut controller = Controller::default();
    controller.begin_frame(&state, TICK);
    assert!(
        controller.tick(&mut state, &physics, Controls::default()),
        "{:?}",
        controller.blocked
    );
    assert!(!controller.body.on_ground);
    assert_eq!(state.player_motion.unwrap().position[1], 1.1 - 0.2);
    assert!(state.player_dynamics.abilities.as_ref().unwrap().flying);
    assert_eq!(controller.pending_ability(), Some((1, 0, true)));
    controller.ability_queued();
    controller.queued();
    controller.begin_frame(&state, TICK);
    assert!(controller.tick(&mut state, &physics, Controls::default()));
    assert!(controller.pending_ability().is_none());
}

#[test]
fn disable_edges_survive_corrections_but_unsent_enables_retire_on_revocation_or_scope_change() {
    let mut state = world(1);
    state.player_dynamics.abilities = Some(world::java::dynamics::Abilities {
        invulnerable: false,
        flying: false,
        may_fly: true,
        creative: false,
        flying_speed: 0.05,
        walking_speed: 0.1,
    });
    let mut controller = Controller::default();
    controller.begin_frame(&state, TICK);
    controller.pending_abilities.extend([(0, false), (0, true)]);
    controller.pending = Some(frame());
    assert!(state.admit(java_protocol::session::Envelope {
        generation: 7,
        event: java_protocol::session::Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: facts::Teleport {
                id: 1,
                position: [3., 64., 4.],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: 0
            },
        }
    }));
    controller.begin_frame(&state, TICK);
    assert!(controller.pending().is_none());
    assert_eq!(controller.pending_ability(), Some((1, 0, false)));
    controller.ability_queued();
    assert_eq!(
        controller.pending_ability(),
        Some((1, 0, true)),
        "valid enable survived the correction"
    );
    state.player_dynamics.abilities.as_mut().unwrap().may_fly = false;
    controller.begin_frame(&state, TICK);
    assert!(
        controller.pending_ability().is_none(),
        "unpermitted unsent enable was retired"
    );
    controller.pending_abilities.push_back((0, false));
    controller.begin_frame(&world(2), TICK);
    assert!(controller.pending_ability().is_none());
}

#[test]
fn block_rollback_retains_a_generated_native_landing_update() {
    let Some((mut state, physics)) = flight_world() else {
        return;
    };
    let mut controller = Controller::default();
    controller.begin_frame(&state, TICK);
    assert!(controller.tick(&mut state, &physics, Controls::default()));
    let stone = physics
        .registry()
        .unwrap()
        .state_id("minecraft:stone", &Default::default())
        .unwrap();
    state.terrain.set_block([8, 1, 8], BlockStateId(stone));
    assert!(
        controller
            .reconcile_block(&mut state, &physics, [8, 1, 8], stone, [8.5, 3., 8.5])
            .unwrap()
    );
    assert!(controller.pending().is_none());
    assert_eq!(controller.pending_ability(), Some((1, 0, false)));
}

#[test]
fn restored_body_position_does_not_release_or_rearm_manual_flight_jump() {
    let Some((mut state, physics)) = flight_world() else {
        return;
    };
    state.player_dynamics.abilities.as_mut().unwrap().flying = false;
    state.player_motion.as_mut().unwrap().position = [8.5, 8., 8.5];
    let mut controller = Controller::default();
    let controls = |jump| Controls {
        travel: Input {
            jump,
            ..Default::default()
        },
        ..Default::default()
    };
    controller.begin_frame(&state, TICK);
    assert!(controller.game_tick(&mut state, Some(&physics), controls(true)));
    assert!(controller.pending_ability().is_none());
    let stone = physics
        .registry()
        .unwrap()
        .state_id("minecraft:stone", &Default::default())
        .unwrap();
    state.terrain.set_block([8, 8, 8], BlockStateId(stone));
    assert!(
        controller
            .reconcile_block(&mut state, &physics, [8, 8, 8], stone, [8.5, 10., 8.5])
            .unwrap()
    );
    // The rollback retires the old pose, not its already consumed logical boundary.
    assert!(controller.pending().is_none());
    assert_eq!(controller.pending_tick_end(), Some(1));
    controller.tick_queued();
    controller.begin_frame(&state, TICK);
    assert!(controller.game_tick(&mut state, Some(&physics), controls(true)));
    assert!(
        controller.pending_ability().is_none(),
        "held Jump is not a second edge after rollback"
    );
    assert!(state.admit(java_protocol::session::Envelope {
        generation: 7,
        event: java_protocol::session::Event::Teleport {
            dimension_epoch: 1,
            sequence: 1,
            correction: facts::Teleport {
                id: 1,
                position: [8.5, 12., 8.5],
                velocity: [0.; 3],
                yaw: 0.,
                pitch: 0.,
                relative: 0,
            },
        },
    }));
    state.correction_queued(1);
    assert!(controller.begin_frame(&state, TICK));
    assert!(controller.pending().is_none());
    assert_eq!(controller.pending_tick_end(), Some(1));
    controller.tick_queued();
    assert!(controller.game_tick(&mut state, Some(&physics), controls(true)));
    assert!(
        controller.pending().is_none(),
        "the queued waiting deadline remains idle"
    );
    assert!(
        controller.pending_ability().is_none(),
        "idle closure does not rearm Jump"
    );
    controller.tick_queued();
    controller.begin_frame(&state, TICK);
    assert!(controller.game_tick(&mut state, Some(&physics), controls(true)));
    assert!(
        controller.pending_ability().is_none(),
        "held Jump survives an ordinary teleport"
    );
    controller.tick_queued();
    controller.begin_frame(&state, TICK);
    assert!(controller.game_tick(&mut state, Some(&physics), controls(false)));
    controller.tick_queued();
    controller.begin_frame(&state, TICK);
    assert!(controller.game_tick(&mut state, Some(&physics), controls(true)));
    assert_eq!(controller.pending_ability(), Some((1, 0, true)));
    assert!(state.player_dynamics.abilities.as_ref().unwrap().flying);
}

#[test]
fn decoded_repeated_grants_retire_obsolete_landing_edges_but_new_jump_is_live() {
    use java_protocol::session::{
        Envelope, Event, abilities::Update, dynamics::Abilities, hud::GameMode,
    };
    let Some((mut state, physics)) = flight_world() else {
        return;
    };
    state.status.mode = Some(world::java::hud::GameMode::Creative);
    let grant = || {
        Update::Values(Abilities {
            invulnerable: true,
            flying: true,
            may_fly: true,
            creative: false,
            flying_speed: 0.05,
            walking_speed: 0.1,
        })
    };
    let admit = |state: &mut WorldState, authority_revision, update| {
        assert!(state.admit(Envelope {
            generation: 7,
            event: Event::Abilities {
                dimension_epoch: 1,
                authority_revision,
                update,
            }
        }));
    };
    // Actual capture order: first flags7 can be admitted while the old mode is Creative.
    admit(&mut state, 1, grant());
    let mut controller = Controller::default();
    controller.begin_frame(&state, TICK);
    assert!(controller.tick(&mut state, &physics, Controls::default()));
    assert_eq!(controller.pending_ability(), Some((1, 1, false)));
    // Mode3 and another flags7 overwrite capabilities before this old edge is written.
    admit(&mut state, 2, Update::Mode(GameMode::Spectator));
    assert!(state.player_dynamics.abilities.as_ref().unwrap().flying);
    admit(&mut state, 3, grant());
    controller.begin_frame(&state, TICK);
    assert!(controller.pending_ability().is_none());
    assert!(state.player_dynamics.abilities.as_ref().unwrap().flying);
    controller.pending_abilities.push_back((3, false));
    // Identical values are a fresh in-place native overwrite, not a no-op revision.
    admit(&mut state, 4, grant());
    controller.begin_frame(&state, TICK);
    assert!(controller.pending_ability().is_none());
    admit(&mut state, 5, Update::Mode(GameMode::Survival));
    let abilities = state.player_dynamics.abilities.as_ref().unwrap();
    assert!(
        !abilities.may_fly && !abilities.flying && !abilities.invulnerable && !abilities.creative
    );
    admit(&mut state, 6, Update::Mode(GameMode::Creative));
    assert!(!state.player_dynamics.abilities.as_ref().unwrap().flying);
    state.player_motion.as_mut().unwrap().position = [8.5, 8., 8.5];
    controller.queued();
    for jump in [true, false, true] {
        controller.begin_frame(&state, TICK);
        assert!(controller.tick(
            &mut state,
            &physics,
            Controls {
                travel: Input {
                    jump,
                    ..Default::default()
                },
                ..Default::default()
            }
        ));
        controller.queued();
    }
    assert_eq!(controller.pending_ability(), Some((1, 6, true)));
    assert!(state.player_dynamics.abilities.as_ref().unwrap().flying);
}
