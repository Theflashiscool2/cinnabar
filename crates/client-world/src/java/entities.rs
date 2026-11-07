//! Admits native entity facts without publishing packet-library types to presentation.
use super::WorldState;
pub(super) mod metadata;
use java_protocol::session::{entities as wire, facts::Relative as Flag};
use std::sync::Arc;
use world::java::{
    entities as domain,
    player::{Motion, Relative},
};

pub(super) fn admit(
    state: &mut WorldState,
    epoch: u64,
    packet: wire::Packet,
) -> Result<bool, &'static str> {
    let scope = (state.generation(), epoch);
    if !state.entities.accepts(scope.0, scope.1) {
        return Ok(false);
    }
    let own = state
        .bootstrap
        .as_ref()
        .ok_or("Java entity before player identity")?
        .world
        .entity_id;
    match packet {
        wire::Packet::Hurt { lifetime, id } => {
            if id == own {
                state.player_visual.hurt_ticks = domain::visual::HURT_TICKS;
                return Ok(true);
            }
            if state.entities.get(id).is_none_or(|e| {
                lifetime == 0 || e.transport_lifetime != lifetime || !metadata::living(&e.kind)
            }) {
                return Ok(false);
            }
            state.entities.update(scope, id, |e| {
                e.visual.hurt_ticks = domain::visual::HURT_TICKS;
                Ok(())
            })
        }
        wire::Packet::Swing {
            lifetime,
            id,
            offhand,
            kind,
            duration,
        } => {
            use domain::animation::swing::{Kind, modified_duration};
            if id == own {
                let modified = modified_duration(&state.player_dynamics.effects, duration);
                state
                    .player_visual
                    .swing
                    .start(offhand, Kind::from_id(kind), duration, modified);
                return Ok(true);
            }
            if state.entities.get(id).is_none_or(|e| {
                lifetime == 0 || e.transport_lifetime != lifetime || !metadata::living(&e.kind)
            }) {
                return Ok(false);
            }
            state.entities.update(scope, id, |e| {
                let modified = modified_duration(&e.visual.effects, duration);
                e.visual
                    .swing
                    .start(offhand, Kind::from_id(kind), duration, modified);
                Ok(())
            })
        }
        wire::Packet::Equipment(packet) => super::equipment::admit(state, scope, packet),
        wire::Packet::Metadata(packet) => metadata::admit(state, scope, own, packet),
        wire::Packet::FinishUsing { id } => {
            if id == own {
                state.player_visual.complete_use()?;
                Ok(true)
            } else {
                state.entities.update(scope, id, |e| {
                    if e.kind.as_ref() == java_protocol::target::ENTITY_PLAYER {
                        e.visual.complete_use()
                    } else {
                        Ok(())
                    }
                })
            }
        }
        wire::Packet::Spawn(s) => {
            if s.id == own {
                return Err("Java remote spawn reuses local player identity");
            }
            let pig = if s.kind == java_protocol::target::ENTITY_PIG {
                Some(metadata::default_pig(state)?)
            } else {
                None
            };
            let villager =
                (s.kind == java_protocol::target::ENTITY_VILLAGER).then(metadata::default_villager);
            let player = if s.kind == java_protocol::target::ENTITY_PLAYER {
                state.players.get(&s.uuid).map(|p| Arc::clone(&p.profile))
            } else {
                None
            };
            let interpolated = pig.is_some()
                || player.is_some()
                || villager.is_some()
                || s.kind == java_protocol::target::ENTITY_ARMOR_STAND;
            let previous = state.entities.get(s.id).map(|e| e.identity);
            let admitted = state.entities.spawn(
                scope,
                domain::Spawn {
                    transport_lifetime: s.lifetime,
                    network_id: s.id,
                    uuid: s.uuid,
                    kind: s.kind.into(),
                    motion: Motion {
                        position: s.position,
                        velocity: s.velocity,
                        yaw: s.yaw,
                        pitch: s.pitch,
                    },
                    head_yaw: s.head_yaw,
                    object_data: s.data,
                    visual: domain::visual::Visual {
                        name_height: java_protocol::target::ENTITY_SPECS
                            [java_protocol::target::ENTITY_TYPES
                                .iter()
                                .position(|n| *n == s.kind)
                                .expect("decoded entity type")]
                        .dimensions[1],
                        armor_stand_flags: (s.kind == java_protocol::target::ENTITY_ARMOR_STAND)
                            .then_some(0),
                        pig,
                        villager,
                        player,
                        ..Default::default()
                    },
                    interpolation_interval: interpolated.then(|| {
                        let id = java_protocol::target::ENTITY_TYPES
                            .iter()
                            .position(|&name| name == s.kind)
                            .expect("decoded entity type");
                        java_protocol::target::ENTITY_SPECS[id].update_interval
                    }),
                },
            )?;
            if admitted && let Some(previous) = previous {
                state.removed_player_modes.remove(&previous);
                state.equipment.remove(previous);
            }
            Ok(admitted)
        }
        wire::Packet::Remove(ids) => {
            let removed = ids
                .iter()
                .filter_map(|id| state.entities.get(*id).map(|e| e.identity))
                .collect::<Vec<_>>();
            let admitted = state.entities.remove(scope, &ids)?;
            if admitted {
                for identity in removed {
                    state.removed_player_modes.remove(&identity);
                    state.equipment.remove(identity);
                }
            }
            Ok(admitted)
        }
        wire::Packet::Move {
            id,
            delta,
            rotation,
            on_ground,
        } => state.entities.update(scope, id, |e| {
            match delta {
                wire::Delta::Linear(delta) => {
                    e.motion.position = domain::decode_delta(e.delta_base, delta);
                    e.path = Arc::from([]);
                }
                wire::Delta::Stepped(steps) => {
                    let mut base = e.delta_base;
                    let mut path = Vec::with_capacity(steps.len());
                    for step in steps {
                        base = domain::decode_delta(base, step.delta);
                        path.push(domain::PositionStep {
                            position: base,
                            tick_offset: step.ticks,
                        });
                    }
                    e.motion.position = base;
                    e.path = path.into();
                }
            }
            e.delta_base = e.motion.position;
            if let Some([yaw, pitch]) = rotation {
                e.motion.yaw = yaw;
                e.motion.pitch = pitch;
            }
            e.on_ground = on_ground;
            e.queue_presentation_motion()
        }),
        wire::Packet::Rotate {
            id,
            rotation: [yaw, pitch],
            on_ground,
        } => state.entities.update(scope, id, |e| {
            e.motion.yaw = yaw;
            e.motion.pitch = pitch;
            e.on_ground = on_ground;
            e.path = Arc::from([]);
            e.queue_presentation_motion()
        }),
        wire::Packet::HeadYaw { id, yaw } => state.entities.update(scope, id, |e| {
            e.head_yaw = yaw;
            e.queue_presentation_head();
            Ok(())
        }),
        wire::Packet::Position {
            id,
            path,
            rotation: [yaw, pitch],
            on_ground,
        } => state.entities.update(scope, id, |e| {
            match path {
                wire::Position::Linear(position) => {
                    e.motion.position = position;
                    e.path = Arc::from([]);
                }
                wire::Position::Stepped(steps) => {
                    e.motion.position = steps
                        .last()
                        .ok_or("empty Java entity position path")?
                        .position;
                    e.path = steps
                        .into_iter()
                        .map(|s| domain::PositionStep {
                            position: s.position,
                            tick_offset: s.tick_offset,
                        })
                        .collect();
                }
            }
            e.delta_base = e.motion.position;
            e.motion.yaw = yaw;
            e.motion.pitch = pitch;
            e.on_ground = on_ground;
            e.synchronize_presentation_motion()
        }),
        wire::Packet::Teleport {
            id,
            position,
            velocity,
            rotation: [yaw, pitch],
            relative,
            on_ground,
        } => {
            let has = |flag: Flag| relative & (1 << flag as u32) != 0;
            state.entities.update(scope, id, |e| {
                e.teleport(
                    Motion {
                        position,
                        velocity,
                        yaw,
                        pitch,
                    },
                    Relative {
                        position: [Flag::X, Flag::Y, Flag::Z].map(has),
                        velocity: [Flag::VelocityX, Flag::VelocityY, Flag::VelocityZ].map(has),
                        yaw: has(Flag::Yaw),
                        pitch: has(Flag::Pitch),
                        rotate_velocity: has(Flag::RotateVelocity),
                    },
                    on_ground,
                )
            })
        }
        wire::Packet::Passengers {
            vehicle,
            passengers,
        } => state
            .entities
            .set_passengers(scope, vehicle, &passengers, own),
    }
}

#[cfg(test)]
mod tests;

pub(super) fn appearances(
    state: &WorldState,
) -> Result<Vec<Arc<domain::visual::PigAppearance>>, &'static str> {
    metadata::appearances(state)
}
