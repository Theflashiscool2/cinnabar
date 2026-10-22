//! Native player dynamics schemas for the pinned protocol; domain application is separate.
use crate::{ProtocolError as Error, target, wire::Reader};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub may_fly: bool,
    pub creative: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Modifier {
    pub name: String,
    pub amount: f64,
    pub operation: u8,
}
#[derive(Clone, Debug, Serialize)]
pub struct Attribute {
    pub name: &'static str,
    pub minimum: f64,
    pub maximum: f64,
    pub base: f64,
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, Serialize)]
pub enum Packet {
    Abilities(Abilities),
    Attributes {
        entity: i32,
        values: Vec<Attribute>,
    },
    Velocity {
        entity: i32,
        velocity: [f64; 3],
    },
    Rotation {
        yaw: f32,
        relative_yaw: bool,
        pitch: f32,
        relative_pitch: bool,
    },
}

pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, Error> {
    Ok(Some(match name {
        "ClientboundPlayerAbilitiesPacket" => {
            let flags = r.byte()?;
            if flags & !15 != 0 {
                return Err(Error::Invalid("Java player ability flags"));
            }
            let flying_speed = r.f32()?;
            let walking_speed = r.f32()?;
            if flying_speed < 0. || walking_speed < 0. {
                return Err(Error::Invalid("negative Java ability speed"));
            }
            Packet::Abilities(Abilities {
                invulnerable: flags & 1 != 0,
                flying: flags & 2 != 0,
                may_fly: flags & 4 != 0,
                creative: flags & 8 != 0,
                flying_speed,
                walking_speed,
            })
        }
        "ClientboundUpdateAttributesPacket" => {
            let entity = r.var()?;
            // The pinned vanilla and maintained codec bound the attribute list to 128.
            let count = r.count(128)?;
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                let id = usize::try_from(r.var()?)
                    .map_err(|_| Error::Invalid("negative Java attribute ID"))?;
                let spec = target::ATTRIBUTES
                    .get(id)
                    .ok_or(Error::Invalid("unknown Java attribute ID"))?;
                let base = r.f64()?;
                // Every modifier needs at least a string length, double and operation byte.
                let count = r.count(r.0.len() / 10)?;
                let mut modifiers = Vec::with_capacity(count);
                let mut names = std::collections::HashSet::new();
                for _ in 0..count {
                    let name = identifier(r.string(32767)?)?;
                    if !names.insert(name.clone()) {
                        return Err(Error::Invalid("duplicate Java attribute modifier"));
                    }
                    let amount = r.f64()?;
                    let operation = r.byte()?;
                    if operation > 2 {
                        return Err(Error::Invalid("Java attribute modifier operation"));
                    }
                    modifiers.push(Modifier {
                        name,
                        amount,
                        operation,
                    });
                }
                values.push(Attribute {
                    name: spec.name,
                    minimum: spec.minimum,
                    maximum: spec.maximum,
                    base,
                    modifiers,
                });
            }
            Packet::Attributes { entity, values }
        }
        "ClientboundSetEntityMotionPacket" => Packet::Velocity {
            entity: r.var()?,
            velocity: velocity(r)?,
        },
        "ClientboundPlayerRotationPacket" => Packet::Rotation {
            yaw: r.f32()?,
            relative_yaw: r.bool()?,
            pitch: r.f32()?,
            relative_pitch: r.bool()?,
        },
        _ => return Ok(None),
    }))
}

/// Network resource identifiers are keys, not filesystem paths. Missing namespaces use vanilla's
/// default; file-loading owners impose their own stronger path traversal restrictions.
fn identifier(value: String) -> Result<String, Error> {
    let (namespace, path) = value.split_once(':').unwrap_or(("minecraft", &value));
    let valid = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b);
    if namespace.is_empty()
        || path.is_empty()
        || !namespace.bytes().all(valid)
        || !path.bytes().all(|b| valid(b) || b == b'/')
    {
        return Err(Error::Invalid("Java modifier resource identifier"));
    }
    Ok(format!("{namespace}:{path}"))
}

/// LpVec3's first two bytes precede a big-endian 32-bit word; its optional scale is unsigned.
fn velocity(r: &mut Reader<'_>) -> Result<[f64; 3], Error> {
    let first = r.byte()?;
    if first == 0 {
        return Ok([0.; 3]);
    }
    let second = r.byte()?;
    let highest = r.i32()? as u32;
    let packed = (u64::from(highest) << 16) | (u64::from(second) << 8) | u64::from(first);
    let mut scale = u64::from(first & 3);
    if first & 4 != 0 {
        scale |= u64::from(r.var()? as u32) << 2;
    }
    Ok([3, 18, 33].map(|shift| {
        let component = ((packed >> shift) & 32767).min(32766) as f64;
        (component * 2. / 32766. - 1.) * scale as f64
    }))
}

#[cfg(test)]
mod tests;
