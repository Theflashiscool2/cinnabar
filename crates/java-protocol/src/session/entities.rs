//! Target-pinned entity lifecycle schemas. World identity and interpolation are domain-owned.
use crate::{ProtocolError as Error, target, wire::Reader};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Spawn {
    pub id: i32,
    pub uuid: [u8; 16],
    pub kind: &'static str,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub pitch: f32,
    pub yaw: f32,
    pub head_yaw: f32,
    pub data: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DeltaStep {
    pub ticks: i32,
    /// Signed fixed-point increments; the domain applies vanilla VecDeltaCodec rounding.
    pub delta: [i16; 3],
}
#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum Delta {
    Linear([i16; 3]),
    Stepped(Vec<DeltaStep>),
}
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PositionStep {
    pub position: [f64; 3],
    pub tick_offset: i32,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
pub enum Position {
    Linear([f64; 3]),
    Stepped(Vec<PositionStep>),
}

#[derive(Clone, Debug, Serialize)]
pub enum Packet {
    Spawn(Spawn),
    Remove(Vec<i32>),
    Move {
        id: i32,
        delta: Delta,
        rotation: Option<[f32; 2]>,
        on_ground: bool,
    },
    Rotate {
        id: i32,
        rotation: [f32; 2],
        on_ground: bool,
    },
    HeadYaw {
        id: i32,
        yaw: f32,
    },
    Position {
        id: i32,
        path: Position,
        rotation: [f32; 2],
        on_ground: bool,
    },
    Teleport {
        id: i32,
        position: [f64; 3],
        velocity: [f64; 3],
        rotation: [f32; 2],
        relative: u32,
        on_ground: bool,
    },
    Passengers {
        vehicle: i32,
        passengers: Vec<i32>,
    },
}

/// Decodes only identified lifecycle packets. Callers must check complete payload consumption.
pub fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, Error> {
    Ok(Some(match name {
        "ClientboundAddEntityPacket" => {
            let id = r.var()?;
            let uuid = r.uuid()?.into_bytes();
            let kind = usize::try_from(r.var()?)
                .ok()
                .and_then(|i| target::ENTITY_TYPES.get(i))
                .ok_or(Error::Invalid("Java entity registry ID"))?;
            Packet::Spawn(Spawn {
                id,
                uuid,
                kind,
                position: vector(r)?,
                velocity: super::dynamics::velocity(r)?,
                pitch: angle(r)?,
                yaw: angle(r)?,
                head_yaw: angle(r)?,
                data: r.var()?,
            })
        }
        "ClientboundRemoveEntitiesPacket" => Packet::Remove(ids(r)?),
        "ClientboundMoveEntityPosPacket" | "ClientboundMoveEntityPosRotPacket" => {
            let id = r.var()?;
            let properties = r.var()? as u32;
            let count = (properties >> 1) as usize;
            let delta = if count == 0 {
                Delta::Linear(short_vector(r)?)
            } else {
                // Vanilla VecDelta.read derives its count bound from the remaining bytes.
                if count > r.0.len() / 7 {
                    return Err(Error::Invalid("Java entity delta steps exceed packet"));
                }
                Delta::Stepped(
                    (0..count)
                        .map(|_| {
                            Ok(DeltaStep {
                                ticks: r.var()?,
                                delta: short_vector(r)?,
                            })
                        })
                        .collect::<Result<_, Error>>()?,
                )
            };
            Packet::Move {
                id,
                delta,
                rotation: if name == "ClientboundMoveEntityPosRotPacket" {
                    Some([angle(r)?, angle(r)?])
                } else {
                    None
                },
                on_ground: properties & 1 != 0,
            }
        }
        "ClientboundMoveEntityRotPacket" => {
            let id = r.var()?;
            let on_ground = r.bool()?;
            Packet::Rotate {
                id,
                on_ground,
                rotation: [angle(r)?, angle(r)?],
            }
        }
        "ClientboundRotateHeadPacket" => Packet::HeadYaw {
            id: r.var()?,
            yaw: angle(r)?,
        },
        "ClientboundEntityPositionSyncPacket" => {
            let id = r.var()?;
            let path = match r.var()? {
                0 => Position::Linear(vector(r)?),
                1 => {
                    let count = r.count(r.0.len() / 25)?;
                    // Vanilla PositionPath.Stepped derives its endpoint from getLast().
                    if count == 0 {
                        return Err(Error::Invalid("empty Java entity position path"));
                    }
                    Position::Stepped(
                        (0..count)
                            .map(|_| {
                                Ok(PositionStep {
                                    position: vector(r)?,
                                    tick_offset: r.var()?,
                                })
                            })
                            .collect::<Result<_, Error>>()?,
                    )
                }
                _ => return Err(Error::Invalid("Java entity position path kind")),
            };
            Packet::Position {
                id,
                path,
                rotation: [r.f32()?, r.f32()?],
                on_ground: r.bool()?,
            }
        }
        "ClientboundTeleportEntityPacket" => {
            let id = r.var()?;
            let position = vector(r)?;
            let velocity = vector(r)?;
            let rotation = [r.f32()?, r.f32()?];
            let relative = r.i32()? as u32;
            if relative & !0x1ff != 0 {
                return Err(Error::Invalid("Java entity teleport relative flags"));
            }
            Packet::Teleport {
                id,
                position,
                velocity,
                rotation,
                relative,
                on_ground: r.bool()?,
            }
        }
        "ClientboundSetPassengersPacket" => Packet::Passengers {
            vehicle: r.var()?,
            passengers: ids(r)?,
        },
        _ => return Ok(None),
    }))
}
fn vector(r: &mut Reader<'_>) -> Result<[f64; 3], Error> {
    Ok([r.f64()?, r.f64()?, r.f64()?])
}
fn short_vector(r: &mut Reader<'_>) -> Result<[i16; 3], Error> {
    let mut values = [0; 3];
    for value in &mut values {
        *value = i16::from_be_bytes(r.take(2)?.try_into().unwrap());
    }
    Ok(values)
}
fn angle(r: &mut Reader<'_>) -> Result<f32, Error> {
    Ok(f32::from(r.byte()? as i8) * 360. / 256.)
}
fn ids(r: &mut Reader<'_>) -> Result<Vec<i32>, Error> {
    let count = r.count(r.0.len())?;
    (0..count).map(|_| r.var()).collect()
}

#[cfg(test)]
mod tests;
