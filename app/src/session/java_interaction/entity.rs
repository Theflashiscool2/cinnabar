//! Selects supported player hitboxes from completed native simulation steps.
use gameplay::java::actions::Target;
use sim::java::{entity_selection, selection, travel};

pub(super) fn pick(
    state: &client_world::java::WorldState,
    from: [f64; 3],
    to: [f64; 3],
    entity_range: f64,
    block_range: f64,
    block: Option<selection::Hit>,
) -> Option<Target> {
    let distance = |p: [f64; 3]| {
        from.into_iter()
            .zip(p)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let mut nearest = block.map_or_else(|| distance(to), |b| distance(b.position));
    let mut selected = block
        .filter(|b| distance(b.position) < block_range)
        .map(Target::Block);
    if state.status.mode == Some(world::java::hud::GameMode::Spectator) {
        return None;
    }
    // Mounted targeting needs native root-vehicle admission, not independent render positions.
    if state.entities.local_vehicle().is_some() {
        return selected;
    }
    for e in state.entities.iter() {
        if e.visual.player.is_none()
            || e.visual.incomplete.is_some()
            || e.visual.scale != 1.
            || state
                .players
                .get(&e.uuid)
                .is_none_or(|p| p.mode == world::java::hud::GameMode::Spectator)
            || state.entities.passengers(e.network_id).is_some()
        {
            continue;
        }
        let crouched = match e.visual.pose.as_ref() {
            "standing" => false,
            "crouching" => true,
            _ => continue,
        };
        let Some(presentation) = &e.presentation else {
            continue;
        };
        // getBoundingBox uses the completed simulation position. Fractional render sampling
        // and the packet's future interpolation target must never enlarge attack reach.
        let bounds = travel::bounds(
            presentation.track.current.position,
            crouched,
            e.visual.scale,
        );
        if let Some(distance) = entity_selection::entry(from, to, bounds, entity_range, nearest) {
            nearest = distance;
            selected = Some(Target::Entity {
                network_id: e.network_id,
                identity: e.identity,
                lifetime: e.transport_lifetime,
            });
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
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
        pick(state, [0., 1.62, 0.], [0., 1.62, 5.], range, 4.5, block)
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
        assert!(matches!(query(&s, 3., Some(block)), Some(Target::Block(_))));
        s.entities
            .update((7, 1), 99, |e| {
                e.visual.pose = "crouching".into();
                Ok(())
            })
            .unwrap();
        assert!(query(&s, 3., None).is_none());
        assert!(matches!(
            pick(&s, [0., 1., 0.], [0., 1., 5.], 3., 4.5, None),
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
        assert!(pick(&s, [0., 1., 0.], [0., 1., 5.], 3., 4.5, None).is_none());
    }
}
