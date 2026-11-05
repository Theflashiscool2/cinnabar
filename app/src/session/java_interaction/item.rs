//! Maps pinned defaults at the application boundary; domain policy resolves server patches.
pub(super) fn resume(
    state: &client_world::java::WorldState,
    hand: gameplay::java::item_use::plan::Hand,
) -> Result<gameplay::java::item_use::plan::Plan, &'static str> {
    plan_for(state, hand, true)
}
fn plan_for(
    state: &client_world::java::WorldState,
    hand: gameplay::java::item_use::plan::Hand,
    authoritative: bool,
) -> Result<gameplay::java::item_use::plan::Plan, &'static str> {
    let held = hand.held(&state.inventory);
    let defaults = held.and_then(|s| s.as_ref()).and_then(|s| {
        let id = java_protocol::target::ITEM_TYPES
            .iter()
            .position(|name| *name == s.item)?;
        let d = java_protocol::target::ITEM_USE_DEFAULTS[id];
        Some(gameplay::java::item_use::Defaults {
            duration: d.duration,
            group: d.group,
        })
    });
    let abilities = state.player_dynamics.abilities.as_ref();
    let arrows = state
        .bootstrap
        .as_ref()
        .and_then(|b| b.configuration.tags.get("minecraft:item"))
        .and_then(|t| t.get("minecraft:arrows"));
    let projectile = (9..inventory::java::PLAYER_MENU_SIZE).any(|slot| {
        state
            .inventory
            .slot(0, slot)
            .and_then(Option::as_ref)
            .is_some_and(|s| {
                java_protocol::target::ITEM_TYPES
                    .iter()
                    .position(|n| *n == s.item)
                    .is_some_and(|id| arrows.is_some_and(|ids| ids.contains(&(id as i32))))
            })
    });
    let empty_cooldowns = inventory::java::cooldown::Cooldowns::default();
    gameplay::java::item_use::plan::prepare(
        held,
        defaults,
        if authoritative {
            &empty_cooldowns
        } else {
            &state.inventory.cooldowns
        },
        gameplay::java::item_use::plan::Conditions {
            food: state.vitals.as_ref().map(|v| v.food),
            invulnerable: authoritative || abilities.is_some_and(|a| a.invulnerable),
            creative: authoritative || abilities.is_some_and(|a| a.creative),
            projectile,
        },
    )
}

/// Resolve wire registry/tag/equipment identities at the application boundary.
fn entity_passes(
    state: &client_world::java::WorldState,
    id: i32,
    hand: gameplay::java::item_use::plan::Hand,
    secondary: bool,
) -> Result<bool, &'static str> {
    use gameplay::java::item_use::entity::{self, Target};
    let target = state
        .entities
        .get(id)
        .ok_or("Native entity target retired")?;
    let held = hand
        .held(&state.inventory)
        .ok_or("Native use hand inventory is not known")?;
    let context = if target.kind.as_ref() == java_protocol::target::ENTITY_PIG {
        let food = if let Some(stack) = held {
            let item = java_protocol::target::ITEM_TYPES
                .iter()
                .position(|n| *n == stack.item)
                .ok_or("Unknown native entity-use item")?;
            state
                .bootstrap
                .as_ref()
                .and_then(|b| b.configuration.tags.get("minecraft:item"))
                .and_then(|t| t.get("minecraft:pig_food"))
                .ok_or("Native pig-food tag is not available")?
                .contains(&(item as i32))
        } else {
            false
        };
        Target::Pig {
            food,
            saddled: state
                .equipment
                .get(target.identity)
                .is_some_and(|e| e.slots[7].is_some()),
            occupied: state
                .entities
                .passengers(id)
                .is_some_and(|p| !p.members.is_empty()),
            secondary,
        }
    } else if target.kind.as_ref() == java_protocol::target::ENTITY_PLAYER {
        Target::Player
    } else {
        Target::Unsupported
    };
    entity::passes(held, context)
}

pub(super) fn steps(
    state: &client_world::java::WorldState,
    target: Option<gameplay::java::actions::Target>,
) -> Result<Vec<gameplay::java::interaction::UseStep>, &'static str> {
    use gameplay::java::{
        actions::Target,
        interaction::{EntityTarget, UseStep},
        item_use::{
            block,
            plan::{Hand, ResultKind},
        },
    };
    let (hit, entity_target, rotation, secondary) = match target {
        Some(Target::Block {
            hit,
            secondary,
            rotation,
        }) => (Some(hit), None, rotation, secondary),
        Some(Target::Air { rotation }) => (None, None, Some(rotation), false),
        Some(Target::Entity {
            network_id,
            identity,
            lifetime,
            location,
            rotation,
            secondary,
        }) => (
            None,
            Some(EntityTarget {
                network_id,
                identity,
                lifetime,
                location,
                secondary,
            }),
            Some(rotation),
            secondary,
        ),
        None => return Err("Native item use requires a loaded targeting snapshot"),
    };
    let mut steps = Vec::with_capacity(4);
    for hand in [Hand::Main, Hand::Off] {
        let slot = hand
            .held(&state.inventory)
            .ok_or("Native use hand inventory is not known")?;
        if let Some(target) = entity_target {
            if !target.current(state) {
                return Ok(Vec::new());
            }
            steps.push(UseStep::entity(target, hand));
            if !entity_passes(state, target.network_id, hand, secondary)? {
                break;
            }
        }
        if let Some(hit) = hit {
            steps.push(UseStep::block(hit, hand));
            if !block::at_hand(state, hit, secondary, hand)? {
                break;
            }
        }
        let plan = plan_for(state, hand, false)?;
        if plan.send {
            steps.push(UseStep::air(
                rotation.ok_or("Native captured use rotation unknown")?,
                hand,
                plan,
                slot,
            )?);
        }
        // Minecraft.startUseItem stops on block FAIL, but air FAIL proceeds to
        // the next hand. An unimplemented specialized result is never guessed PASS.
        if matches!(
            plan.result,
            ResultKind::Consume(_) | ResultKind::Unpredicted
        ) {
            break;
        }
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    #[test]
    fn pinned_native_use_defaults_distinguish_instant_and_active_items() {
        use java_protocol::target;
        assert_eq!(target::ITEM_TYPES.len(), target::ITEM_USE_DEFAULTS.len());
        let duration = |name| {
            target::ITEM_USE_DEFAULTS[target::ITEM_TYPES.iter().position(|n| *n == name).unwrap()]
                .duration
        };
        assert_eq!(duration("minecraft:compass"), 0);
        assert_eq!(duration("minecraft:snowball"), 0);
        assert_eq!(duration("minecraft:bread"), 32);
        assert_eq!(duration("minecraft:bow"), 72000);
    }
    fn use_state(
        main: Option<&str>,
        off: Option<&str>,
        food: i32,
    ) -> client_world::java::WorldState {
        use inventory::java::{Stack, Update};
        let mut s = client_world::java::WorldState::default();
        s.begin(7);
        s.terrain.begin(world::java::Dimension {
            session: 7,
            epoch: 1,
            name: "fixture:use".into(),
            min_y: -64,
            height: 384,
            has_skylight: true,
        });
        s.status.mode = Some(world::java::hud::GameMode::Survival);
        s.vitals = Some(java_protocol::session::facts::Vitals {
            health: 20.,
            food,
            saturation: 0.,
        });
        s.inventory.begin(7, 1);
        for (slot, name) in [(0, main), (40, off)] {
            s.inventory
                .admit(
                    7,
                    1,
                    Update::PlayerSlot {
                        slot,
                        stack: name.map(|name| Stack {
                            item: name.into(),
                            count: 2,
                            added: Default::default(),
                            removed: Default::default(),
                        }),
                    },
                )
                .unwrap();
        }
        s
    }
    fn requests(
        s: &client_world::java::WorldState,
    ) -> (
        Vec<gameplay::java::item_use::plan::Hand>,
        Option<gameplay::java::item_use::plan::Hand>,
    ) {
        use gameplay::java::{actions::Target, interaction::Controller};
        let steps = super::steps(
            s,
            Some(Target::Air {
                rotation: [37., -12.],
            }),
        )
        .unwrap();
        let mut c = Controller::default();
        c.begin_frame(
            s,
            std::time::Duration::ZERO,
            Some(semantic_input::identity::InputAuthorityGeneration::INITIAL),
            false,
        );
        c.use_chain(
            s,
            semantic_input::ActionPhase {
                pressed: true,
                held: true,
                released: false,
            },
            Ok(steps),
        );
        let mut hands = Vec::new();
        while let Some(request) = c.pending() {
            assert_eq!(
                request.target,
                gameplay::java::interaction::Target::Air([37., -12.])
            );
            hands.push(request.hand);
            c.queued();
        }
        (hands, c.using_hand())
    }
    #[test]
    fn native_main_pass_empty_and_air_fail_continue_to_real_offhand_but_consumption_stops() {
        use gameplay::java::item_use::plan::Hand::{Main, Off};
        assert_eq!(
            requests(&use_state(
                Some("minecraft:iron_sword"),
                Some("minecraft:apple"),
                6
            )),
            (vec![Main, Off], Some(Off))
        );
        assert_eq!(
            requests(&use_state(None, Some("minecraft:apple"), 6)),
            (vec![Off], Some(Off))
        );
        assert_eq!(
            requests(&use_state(
                Some("minecraft:apple"),
                Some("minecraft:shield"),
                20
            )),
            (vec![Main, Off], Some(Off)),
            "air FAIL is not the block FAIL early return"
        );
        assert_eq!(
            requests(&use_state(
                Some("minecraft:apple"),
                Some("minecraft:shield"),
                6
            )),
            (vec![Main], Some(Main))
        );
        assert_eq!(
            requests(&use_state(
                Some("minecraft:snowball"),
                Some("minecraft:apple"),
                6
            )),
            (vec![Main], None),
            "unknown specialized result is never guessed PASS"
        );
    }
    #[test]
    fn entity_fallthrough_sends_both_interactions_before_starting_offhand_food() {
        use gameplay::java::{
            actions::Target,
            interaction::{Controller, Kind},
            item_use::plan::Hand,
        };
        use world::java::{
            entities::{Spawn, visual::Visual},
            player::Motion,
        };
        let mut s = use_state(Some("minecraft:iron_sword"), Some("minecraft:carrot"), 6);
        s.entities.begin(7, 1);
        s.entities
            .spawn(
                (7, 1),
                Spawn {
                    transport_lifetime: 8,
                    network_id: 42,
                    uuid: [1; 16],
                    kind: java_protocol::target::ENTITY_PLAYER.into(),
                    motion: Motion::default(),
                    head_yaw: 0.,
                    object_data: 0,
                    visual: Visual::default(),
                    interpolation_interval: Some(3),
                },
            )
            .unwrap();
        let target = Target::Entity {
            network_id: 42,
            identity: s.entities.get(42).unwrap().identity,
            lifetime: 8,
            location: [0., 1.5, 0.],
            rotation: [0.; 2],
            secondary: false,
        };
        let steps = super::steps(&s, Some(target)).unwrap();
        let mut c = Controller::default();
        c.begin_frame(
            &s,
            std::time::Duration::ZERO,
            Some(semantic_input::identity::InputAuthorityGeneration::INITIAL),
            false,
        );
        c.use_chain(
            &s,
            semantic_input::ActionPhase {
                pressed: true,
                held: true,
                released: false,
            },
            Ok(steps),
        );
        for (kind, hand) in [
            (Kind::UseEntity, Hand::Main),
            (Kind::UseAir, Hand::Main),
            (Kind::UseEntity, Hand::Off),
            (Kind::UseAir, Hand::Off),
        ] {
            let request = c.pending().unwrap();
            assert_eq!((request.kind, request.hand), (kind, hand));
            c.queued();
        }
        assert!(c.pending().is_none());
        assert_eq!(c.using_hand(), Some(Hand::Off));
        assert_eq!(c.entity_uses_queued, 2);
        s.entities.remove((7, 1), &[42]).unwrap();
        assert!(super::steps(&s, Some(target)).unwrap().is_empty());
    }
}
