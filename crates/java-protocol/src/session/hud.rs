//! Native status schemas. Packet and enum identities come from the pinned codec inventory.
use crate::{ProtocolError, target, wire::Reader};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}
impl GameMode {
    /// Matches the target's byId fallback for out-of-range, well-framed modes.
    pub fn from_id(id: i32) -> Self {
        match u8::try_from(id).ok() {
            Some(target::GAME_MODE_CREATIVE) => Self::Creative,
            Some(target::GAME_MODE_ADVENTURE) => Self::Adventure,
            Some(target::GAME_MODE_SPECTATOR) => Self::Spectator,
            _ => Self::Survival,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Packet {
    Experience {
        progress: f32,
        level: i32,
        total: i32,
    },
    GameMode(GameMode),
    /// Decoded notifications with no implemented presentation or gameplay behavior yet.
    Notification {
        kind: &'static str,
        value: f32,
    },
}
pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Result<Option<Packet>, ProtocolError> {
    Ok(Some(match name {
        "ClientboundSetExperiencePacket" => Packet::Experience {
            progress: r.f32()?,
            level: r.var()?,
            total: r.var()?,
        },
        "ClientboundGameEventPacket" => {
            let id = r.byte()?;
            let value = r.f32()?;
            let kind = *target::GAME_EVENT_NAMES
                .get(id as usize)
                .ok_or(ProtocolError::Invalid("unknown Java game event"))?;
            if id == target::GAME_EVENT_CHANGE_GAME_MODE {
                Packet::GameMode(GameMode::from_id(value as i32))
            } else {
                Packet::Notification { kind, value }
            }
        }
        _ => return Ok(None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_hud_wire_vectors_preserve_order_and_reject_truncation() {
        // Float 0.5, VarInt level 300, VarInt total 1000.
        let bytes = [0x3f, 0, 0, 0, 0xac, 2, 0xe8, 7];
        let mut r = Reader(&bytes);
        assert!(matches!(
            decode("ClientboundSetExperiencePacket", &mut r).unwrap(),
            Some(Packet::Experience {
                progress: 0.5,
                level: 300,
                total: 1000
            })
        ));
        r.finish().unwrap();
        for end in 0..bytes.len() {
            assert!(decode("ClientboundSetExperiencePacket", &mut Reader(&bytes[..end])).is_err());
        }
        // Independent protocol notification 3, float mode 2 = adventure.
        assert!(matches!(
            decode(
                "ClientboundGameEventPacket",
                &mut Reader(&[3, 0x40, 0, 0, 0])
            )
            .unwrap(),
            Some(Packet::GameMode(GameMode::Adventure))
        ));
        assert!(
            decode(
                "ClientboundGameEventPacket",
                &mut Reader(&[255, 0, 0, 0, 0])
            )
            .is_err()
        );
        assert_eq!(GameMode::from_id(-1), GameMode::Survival);
        assert_eq!(GameMode::from_id(256), GameMode::Survival);
    }
}
