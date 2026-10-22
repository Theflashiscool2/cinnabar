//! Admits native entity facts without publishing packet-library types to presentation.
use super::WorldState;
mod metadata;
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
        wire::Packet::Metadata(packet) => metadata::admit(state, scope, own, packet),
        wire::Packet::Spawn(s) => {
            if s.id == own {
                return Err("Java remote spawn reuses local player identity");
            }
            let pig = if s.kind == java_protocol::target::ENTITY_PIG {
                Some(metadata::default_pig(state)?)
            } else {
                None
            };
            state.entities.spawn(
                scope,
                domain::Spawn {
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
                        pig,
                        ..Default::default()
                    },
                },
            )
        }
        wire::Packet::Remove(ids) => state.entities.remove(scope, &ids),
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
            Ok(())
        }),
        wire::Packet::Rotate {
            id,
            rotation: [yaw, pitch],
            on_ground,
        } => state.entities.update(scope, id, |e| {
            e.motion.yaw = yaw;
            e.motion.pitch = pitch;
            e.on_ground = on_ground;
            Ok(())
        }),
        wire::Packet::HeadYaw { id, yaw } => state.entities.update(scope, id, |e| {
            e.head_yaw = yaw;
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
            Ok(())
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
