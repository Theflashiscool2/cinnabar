//! Admits local player dynamics without leaking Java packet types into simulation.
use super::WorldState;
use java_protocol::session::dynamics as wire;
use world::java::{
    dynamics as domain,
    player::{Motion, Relative},
};

pub(super) fn admit(
    state: &mut WorldState,
    epoch: u64,
    packet: wire::Packet,
) -> Result<bool, &'static str> {
    if !state.terrain.accepts(state.generation(), epoch) {
        return Ok(false);
    }
    let own_id = state
        .bootstrap
        .as_ref()
        .ok_or("Java dynamics before player identity")?
        .world
        .entity_id;
    match packet {
        wire::Packet::Abilities(a) => {
            state.player_dynamics.abilities = Some(domain::Abilities {
                invulnerable: a.invulnerable,
                flying: a.flying,
                may_fly: a.may_fly,
                creative: a.creative,
                flying_speed: a.flying_speed,
                walking_speed: a.walking_speed,
            })
        }
        wire::Packet::Attributes { entity, values } if entity == own_id => {
            let values = values
                .into_iter()
                .map(|a| {
                    let attribute = domain::Attribute {
                        base: a.base,
                        minimum: a.minimum,
                        maximum: a.maximum,
                        modifiers: a
                            .modifiers
                            .into_iter()
                            .map(|m| {
                                Ok(domain::Modifier {
                                    name: m.name,
                                    amount: m.amount,
                                    operation: match m.operation {
                                        0 => domain::Operation::Add,
                                        1 => domain::Operation::AddMultipliedBase,
                                        2 => domain::Operation::AddMultipliedTotal,
                                        _ => return Err("invalid Java attribute operation"),
                                    },
                                })
                            })
                            .collect::<Result<_, _>>()?,
                    };
                    attribute.value()?;
                    Ok((a.name.to_owned(), attribute))
                })
                .collect::<Result<Vec<_>, &'static str>>()?;
            state.player_dynamics.attributes.extend(values);
        }
        wire::Packet::Velocity { entity, velocity } if entity == own_id => {
            if !velocity.into_iter().all(f64::is_finite) {
                return Err("nonfinite Java player velocity");
            }
            if let Some(motion) = &mut state.player_motion {
                motion.velocity = velocity;
            } else {
                state.player_dynamics.unplaced_velocity = Some(velocity);
            }
        }
        wire::Packet::Rotation {
            yaw,
            relative_yaw,
            pitch,
            relative_pitch,
        } => {
            let before = state
                .player_motion
                .ok_or("Java rotation before player position")?;
            let motion = before.corrected(
                Motion {
                    yaw,
                    pitch,
                    ..before
                },
                Relative {
                    yaw: relative_yaw,
                    pitch: relative_pitch,
                    ..Default::default()
                },
            )?;
            state.player_pose_revision = state
                .player_pose_revision
                .checked_add(1)
                .ok_or("Java player pose revision overflow")?;
            state.player_motion = Some(motion);
        }
        wire::Packet::Attributes { .. } | wire::Packet::Velocity { .. } => {
            // Remote lifecycle is still unsupported. Do not admit remote IDs as the local player.
            state.deferred_entity_dynamics = state.deferred_entity_dynamics.saturating_add(1);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
