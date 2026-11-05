//! Selects supported player/pig hitboxes from completed native simulation steps.
use gameplay::java::actions::Target;
use sim::java::{entity_selection, selection, travel};

/// Resolves pinned native defaults at the host boundary; the gameplay policy owns patches.
pub(super) fn ordinary_attack(held: Option<&inventory::java::Slot>) -> Result<(), &'static str> {
    let defaults = held.and_then(|h| h.as_ref()).and_then(|stack| {
        let id = java_protocol::target::ITEM_TYPES
            .iter()
            .position(|name| *name == stack.item)?;
        let d = java_protocol::target::ITEM_ATTACK_DEFAULTS[id];
        Some(gameplay::java::attack::Defaults {
            range: d.range,
            piercing: d.piercing,
            minimum_charge: d.minimum_charge,
        })
    });
    gameplay::java::attack::ordinary(held, defaults)
}

pub(super) fn pick(
    state: &client_world::java::WorldState,
    from: [f64; 3],
    to: [f64; 3],
    entity_range: f64,
    block_range: f64,
    block: Option<selection::Hit>,
    context: ([f32; 2], bool),
) -> Option<Target> {
    let distance = |p| distance_to(from, p);
    if distance(to) == 0. {
        return None;
    }
    let mut nearest = block.map_or_else(|| distance(to), |b| distance(b.position));
    let mut selected = block
        .filter(|b| distance(b.position) < block_range)
        .map(|hit| Target::Block {
            hit,
            secondary: context.1,
            rotation: Some(context.0),
        });
    if state.status.mode == Some(world::java::hud::GameMode::Spectator) {
        return None;
    }
    // Mounted targeting needs native root-vehicle admission, not independent render positions.
    if state.entities.local_vehicle().is_some() {
        return selected;
    }
    for e in state.entities.iter() {
        let player = e.visual.player.is_some();
        let pig = e.visual.pig.is_some();
        if (!player && !pig)
            || e.visual.incomplete.is_some()
            || e.visual.health.is_some_and(|h| h <= 0.)
            || e.visual.scale != 1.
            || (player
                && state
                    .player_entity_mode(e.network_id)
                    .is_some_and(|mode| mode == world::java::hud::GameMode::Spectator))
            || state.entities.passengers(e.network_id).is_some()
        {
            continue;
        }
        let Some(presentation) = &e.presentation else {
            continue;
        };
        let position = presentation.track.current.position;
        // Both rendering and targeting follow the completed native interpolation step.
        let bounds = if player {
            let crouched = match e.visual.pose.as_ref() {
                "standing" => false,
                "crouching" => true,
                _ => continue,
            };
            travel::bounds(position, crouched, e.visual.scale)
        } else {
            if e.visual.pose.as_ref() != "standing" {
                continue;
            }
            let id = java_protocol::target::ENTITY_TYPES
                .iter()
                .position(|n| *n == e.kind.as_ref())?;
            let dimensions = java_protocol::target::ENTITY_SPECS[id].dimensions;
            let scale = if e.visual.baby { 0.5 } else { 1. };
            let width = f64::from(dimensions[0] * scale);
            let height = f64::from(dimensions[1] * scale);
            assets::registry::java::physics::Bounds {
                min: [
                    position[0] - width / 2.,
                    position[1],
                    position[2] - width / 2.,
                ],
                max: [
                    position[0] + width / 2.,
                    position[1] + height,
                    position[2] + width / 2.,
                ],
            }
        };
        if let Some(distance) = entity_selection::entry(from, to, bounds, entity_range, nearest) {
            nearest = distance;
            selected = Some(Target::Entity {
                network_id: e.network_id,
                identity: e.identity,
                lifetime: e.transport_lifetime,
                location: {
                    let length = distance_to(from, to);
                    std::array::from_fn(|i| {
                        from[i] + (to[i] - from[i]) * distance / length - position[i]
                    })
                },
                rotation: context.0,
                secondary: context.1,
            });
        }
    }
    selected
}

fn distance_to(from: [f64; 3], to: [f64; 3]) -> f64 {
    from.into_iter()
        .zip(to)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_defaults_dispatch_ordinary_items_and_reject_native_special_attacks() {
        let item = |name: &str| {
            Some(inventory::java::Stack {
                item: name.into(),
                count: 1,
                added: Default::default(),
                removed: Default::default(),
            })
        };
        assert_eq!(
            java_protocol::target::ITEM_TYPES.len(),
            java_protocol::target::ITEM_ATTACK_DEFAULTS.len()
        );
        for name in [
            "minecraft:iron_sword",
            "minecraft:diamond_axe",
            "minecraft:gold_ingot",
        ] {
            assert!(ordinary_attack(Some(&item(name))).is_ok(), "{name}");
        }
        assert!(ordinary_attack(Some(&item("minecraft:iron_spear"))).is_err());
        assert!(ordinary_attack(Some(&item("fixture:unknown"))).is_err());
    }
    use std::sync::Arc;
    use world::java::{entities, hud::GameMode, player::Motion, roster};
    fn state() -> client_world::java::WorldState {
        let mut state = client_world::java::WorldState::default();
        state.begin(7);
        state.status.mode = Some(GameMode::Survival);
        state.entities.begin(7, 1);
        let profile = Arc::new(roster::Profile {
            name: "Target".into(),
            properties: Arc::from([]),
            default_skin: roster::DefaultSkin {
                texture: "minecraft:fixture".into(),
                slim: false,
            },
        });
        assert!(state.players.update(
            7,
            vec![roster::Update {
                uuid: [1; 16],
                profile: Some(profile.clone()),
                mode: Some(GameMode::Survival),
                ..Default::default()
            }]
        ));
        state
            .entities
            .spawn(
                (7, 1),
                entities::Spawn {
                    transport_lifetime: 9,
                    network_id: 99,
                    uuid: [1; 16],
                    kind: "minecraft:player".into(),
                    motion: Motion {
                        position: [0., 0., 3.],
                        ..Default::default()
                    },
                    head_yaw: 0.,
                    object_data: 0,
                    visual: entities::visual::Visual {
                        player: Some(profile),
                        ..Default::default()
                    },
                    interpolation_interval: Some(3),
                },
            )
            .unwrap();
        state
    }
    fn query(
        state: &client_world::java::WorldState,
        range: f64,
        block: Option<selection::Hit>,
    ) -> Option<Target> {
        pick(
            state,
            [0., 1.62, 0.],
            [0., 1.62, 5.],
            range,
            4.5,
            block,
            ([0., 0.], false),
        )
    }
    #[test]
    fn removing_a_tab_entry_does_not_make_a_visible_player_untargetable() {
        let mut s = state();
        assert!(s.players.remove(7, &[[1; 16]]));
        assert!(s.entities.get(99).unwrap().visual.player.is_some());
        assert!(matches!(
            query(&s, 3., None),
            Some(Target::Entity { network_id: 99, .. })
        ));
    }
    #[test]
    fn current_native_boxes_not_future_packets_or_cosmetic_offsets_define_reach() {
        let mut s = state();
        assert!(matches!(
            query(&s, 3., None),
            Some(Target::Entity {
                network_id: 99,
                lifetime: 9,
                ..
            })
        ));
        s.entities
            .update((7, 1), 99, |e| {
                e.motion.position = [0., 0., 1.];
                e.queue_presentation_motion()
            })
            .unwrap();
        assert!(query(&s, 2., None).is_none());
        s.entities.tick_presentation();
        assert!(matches!(query(&s, 3., None), Some(Target::Entity { .. })));
        s.entities.remove((7, 1), &[99]).unwrap();
        assert!(query(&s, 3., None).is_none());
    }
    #[test]
    fn occlusion_crouch_and_spectator_use_native_facts() {
        let mut s = state();
        let block = selection::Hit {
            block: [0, 1, 2],
            state: 42,
            position: [0., 1.62, 2.],
            face: selection::Face::North,
            inside: false,
        };
        assert!(matches!(
            query(&s, 3., Some(block)),
            Some(Target::Block { .. })
        ));
        s.entities
            .update((7, 1), 99, |e| {
                e.visual.pose = "crouching".into();
                Ok(())
            })
            .unwrap();
        assert!(query(&s, 3., None).is_none());
        assert!(matches!(
            pick(
                &s,
                [0., 1., 0.],
                [0., 1., 5.],
                3.,
                4.5,
                None,
                ([0., 0.], false)
            ),
            Some(Target::Entity { .. })
        ));
        s.players.update(
            7,
            vec![roster::Update {
                uuid: [1; 16],
                mode: Some(GameMode::Spectator),
                ..Default::default()
            }],
        );
        assert!(
            pick(
                &s,
                [0., 1., 0.],
                [0., 1., 5.],
                3.,
                4.5,
                None,
                ([0., 0.], false)
            )
            .is_none()
        );
    }
    #[test]
    fn pig_hit_point_uses_native_dimensions_baby_height_and_click_context() {
        let mut s = state();
        s.entities.remove((7, 1), &[99]).unwrap();
        s.entities
            .spawn(
                (7, 1),
                entities::Spawn {
                    transport_lifetime: 10,
                    network_id: 99,
                    uuid: [2; 16],
                    kind: "minecraft:pig".into(),
                    motion: Motion {
                        position: [0., 0., 3.],
                        ..Default::default()
                    },
                    head_yaw: 0.,
                    object_data: 0,
                    visual: entities::visual::Visual {
                        pig: Some(Arc::new(entities::visual::PigAppearance {
                            name: "fixture:normal".into(),
                            model: entities::visual::PigModel::Normal,
                            adult_texture: "fixture:pig".into(),
                            baby_texture: "fixture:baby".into(),
                        })),
                        ..Default::default()
                    },
                    interpolation_interval: Some(3),
                },
            )
            .unwrap();
        let hit = pick(
            &s,
            [0., 0.7, 0.],
            [0., 0.7, 5.],
            3.,
            4.5,
            None,
            ([0., 20.], true),
        );
        let Some(Target::Entity {
            location,
            rotation,
            secondary,
            ..
        }) = hit
        else {
            panic!("visible adult pig should be selectable")
        };
        assert!((location[2] + 0.44999998807907104).abs() < 1e-10);
        assert_eq!(location[0], 0.);
        assert_eq!(location[1], 0.7);
        assert_eq!(rotation, [0., 20.]);
        assert!(secondary);
        s.entities
            .update((7, 1), 99, |e| {
                e.visual.baby = true;
                Ok(())
            })
            .unwrap();
        assert!(
            pick(
                &s,
                [0., 0.7, 0.],
                [0., 0.7, 5.],
                3.,
                4.5,
                None,
                ([0., 0.], false)
            )
            .is_none()
        );
        assert!(
            pick(
                &s,
                [0., 0.2, 0.],
                [0., 0.2, 5.],
                3.,
                4.5,
                None,
                ([0., 0.], false)
            )
            .is_some()
        );
        s.entities
            .update((7, 1), 99, |e| {
                e.visual.health = Some(0.);
                Ok(())
            })
            .unwrap();
        assert!(
            pick(
                &s,
                [0., 0.2, 0.],
                [0., 0.2, 5.],
                3.,
                4.5,
                None,
                ([0., 0.], false)
            )
            .is_none()
        );
    }
    #[test]
    fn entity_hit_location_is_translation_invariant_across_negative_boundaries() {
        let s = state();
        let Some(Target::Entity {
            location: before, ..
        }) = query(&s, 3., None)
        else {
            panic!()
        };
        let mut translated = state();
        translated
            .entities
            .update((7, 1), 99, |e| {
                e.motion.position = [-32., -64., -13.];
                e.presentation.as_mut().unwrap().track.current.position = e.motion.position;
                Ok(())
            })
            .unwrap();
        let Some(Target::Entity {
            location: after, ..
        }) = pick(
            &translated,
            [-32., -62.38, -16.],
            [-32., -62.38, -11.],
            3.,
            4.5,
            None,
            ([0., 0.], false),
        )
        else {
            panic!()
        };
        for (a, b) in before.into_iter().zip(after) {
            assert!((a - b).abs() < 1e-10, "{before:?} != {after:?}");
        }
    }
}
