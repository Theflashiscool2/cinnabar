//! Native block actions. Action IDs derive from the pinned enum, including changed directions.
use crate::{ProtocolError as Error, codec::write_varint, target};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    ChangeDirection,
    Abort,
    Finish,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}
#[derive(Clone, Copy, Debug)]
pub struct Dig {
    pub action: Action,
    pub position: [i32; 3],
    pub face: Face,
    pub sequence: i32,
}
#[derive(Clone, Copy, Debug)]
pub enum Hand {
    Main,
    Off,
}
#[derive(Clone, Copy, Debug)]
pub struct UseOn {
    pub hand: Hand,
    pub position: [i32; 3],
    pub face: Face,
    /// Hit location relative to the addressed block, as in the native BlockHitResult codec.
    pub cursor: [f32; 3],
    pub inside: bool,
    pub world_border: bool,
    pub sequence: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct UseItem {
    pub hand: Hand,
    pub sequence: i32,
    pub rotation: [f32; 2],
}
/// Native hand, prediction sequence, yaw and pitch; no fabricated block hit or use result.
pub fn encode_use_item(action: UseItem) -> Result<Vec<u8>, Error> {
    if action.sequence < 0 || action.rotation.iter().any(|v| !v.is_finite()) {
        return Err(Error::Invalid("Java item use sequence/rotation"));
    }
    let mut bytes = Vec::with_capacity(14);
    write_varint(
        i32::from(match action.hand {
            Hand::Main => target::HAND_MAIN_HAND,
            Hand::Off => target::HAND_OFF_HAND,
        }),
        &mut bytes,
    );
    write_varint(action.sequence, &mut bytes);
    for angle in action.rotation {
        bytes.extend(angle.to_be_bytes());
    }
    Ok(bytes)
}
/// Java 26.3 separates attack from interaction: the payload is only the entity VarInt.
pub fn encode_attack(entity: i32) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::with_capacity(5);
    write_varint(entity, &mut bytes);
    Ok(bytes)
}
/// The26.3 interact packet includes the hit location directly (no legacy interact-at opcode).
#[derive(Clone, Copy, Debug)]
pub struct EntityUse {
    pub entity: i32,
    pub hand: Hand,
    pub location: [f64; 3],
    pub secondary: bool,
}
pub fn encode_entity_use(action: EntityUse) -> Result<Vec<u8>, Error> {
    // Location is entity-relative, not a world position or a fabricated centre point.
    if action
        .location
        .iter()
        .any(|c| !c.is_finite() || c.abs() > 1.717_986_918_3e10)
    {
        return Err(Error::Invalid("invalid Java entity interaction point"));
    }
    let mut out = Vec::with_capacity(16);
    write_varint(action.entity, &mut out);
    write_varint(
        i32::from(match action.hand {
            Hand::Main => target::HAND_MAIN_HAND,
            Hand::Off => target::HAND_OFF_HAND,
        }),
        &mut out,
    );
    // MIT LpVec3 packing adapted from horizon-protocol/buf.rs at59d8cc38;
    // full notice assets/licenses/Native-Java-Reference-MIT.txt. Verified against official26.3.
    let length = action.location.into_iter().map(f64::abs).fold(0., f64::max);
    if length < 3.051_944_088_384_301e-5 {
        out.push(0);
    } else {
        let scale = length.ceil() as u64;
        let extended = scale > 3;
        let mut packed = if extended { scale & 3 | 4 } else { scale };
        for (component, shift) in action.location.into_iter().zip([3, 18, 33]) {
            let quantized = ((component / scale as f64 * 0.5 + 0.5) * 32766.).round() as u64;
            packed |= quantized << shift;
        }
        out.extend([packed as u8, (packed >> 8) as u8]);
        out.extend(((packed >> 16) as u32).to_be_bytes());
        if extended {
            write_varint((scale >> 2) as i32, &mut out);
        }
    }
    out.push(u8::from(action.secondary));
    Ok(out)
}
/// Encodes the pinned use-on-block schema; it does not imply placement succeeded.
pub fn encode_use_on(action: UseOn) -> Result<Vec<u8>, Error> {
    let position = packed_position(action.position, action.sequence)?;
    if action.cursor.iter().any(|v| !v.is_finite()) {
        return Err(Error::Invalid("nonfinite Java block hit"));
    }
    let mut bytes = Vec::with_capacity(30);
    write_varint(
        i32::from(match action.hand {
            Hand::Main => target::HAND_MAIN_HAND,
            Hand::Off => target::HAND_OFF_HAND,
        }),
        &mut bytes,
    );
    bytes.extend(position.to_be_bytes());
    write_varint(i32::from(face_id(action.face)), &mut bytes);
    for coordinate in action.cursor {
        bytes.extend(coordinate.to_be_bytes());
    }
    bytes.extend([u8::from(action.inside), u8::from(action.world_border)]);
    write_varint(action.sequence, &mut bytes);
    Ok(bytes)
}
/// The caller owns sequence allocation and edition/session selection.
pub fn encode(dig: Dig) -> Result<Vec<u8>, Error> {
    let position = packed_position(dig.position, dig.sequence)?;
    let action = match dig.action {
        Action::Start => target::PLAYER_ACTION_START_DIGGING,
        Action::ChangeDirection => target::PLAYER_ACTION_CHANGE_DIGGING_DIRECTION,
        Action::Abort => target::PLAYER_ACTION_CANCEL_DIGGING,
        Action::Finish => target::PLAYER_ACTION_FINISH_DIGGING,
    };
    let mut bytes = Vec::with_capacity(15);
    write_varint(i32::from(action), &mut bytes);
    bytes.extend(position.to_be_bytes());
    bytes.push(face_id(dig.face));
    write_varint(dig.sequence, &mut bytes);
    Ok(bytes)
}

fn face_id(face: Face) -> u8 {
    match face {
        Face::Down => target::BLOCK_FACE_DOWN,
        Face::Up => target::BLOCK_FACE_UP,
        Face::North => target::BLOCK_FACE_NORTH,
        Face::South => target::BLOCK_FACE_SOUTH,
        Face::West => target::BLOCK_FACE_WEST,
        Face::East => target::BLOCK_FACE_EAST,
    }
}
fn packed_position([x, y, z]: [i32; 3], sequence: i32) -> Result<u64, Error> {
    if !(-(1 << 25)..(1 << 25)).contains(&x)
        || !(-(1 << 25)..(1 << 25)).contains(&z)
        || !(-2048..2048).contains(&y)
        || sequence < 0
    {
        return Err(Error::Invalid("Java block action coordinate/sequence"));
    }
    Ok(((x as u64 & 0x3ff_ffff) << 38) | ((z as u64 & 0x3ff_ffff) << 12) | (y as u64 & 0xfff))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_air_use_vector_has_hand_sequence_and_captured_angles_without_a_block() {
        let action = UseItem {
            hand: Hand::Main,
            sequence: 129,
            rotation: [90., -45.],
        };
        assert_eq!(
            encode_use_item(action).unwrap(),
            [0, 0x81, 1, 0x42, 0xb4, 0, 0, 0xc2, 0x34, 0, 0]
        );
        assert_eq!(
            encode_use_item(UseItem {
                hand: Hand::Off,
                ..action
            })
            .unwrap()[0],
            1
        );
        assert!(
            encode_use_item(UseItem {
                sequence: -1,
                ..action
            })
            .is_err()
        );
        assert!(
            encode_use_item(UseItem {
                rotation: [f32::NAN, 0.],
                ..action
            })
            .is_err()
        );
        assert!(
            encode_use_item(UseItem {
                rotation: [0., f32::INFINITY],
                ..action
            })
            .is_err()
        );
    }
    #[test]
    fn native_attack_is_only_entity_varint_without_legacy_action_or_sneak_fields() {
        for (entity, bytes) in [
            (0, vec![0]),
            (15, vec![15]),
            (127, vec![127]),
            (128, vec![128, 1]),
            (300, vec![172, 2]),
            (-1, vec![255, 255, 255, 255, 15]),
        ] {
            assert_eq!(encode_attack(entity).unwrap(), bytes);
        }
    }
    #[test]
    fn native_use_on_vector_keeps_negative_position_fractional_hit_and_both_flags() {
        // Independent native packet field order and IEEE-754 values. Protocol ID is tested
        // separately through the encrypted play session against the official packet report.
        let action = UseOn {
            hand: Hand::Main,
            position: [-1, -64, -2],
            face: Face::North,
            cursor: [0.5, 1., 0.25],
            inside: true,
            world_border: false,
            sequence: 129,
        };
        assert_eq!(
            encode_use_on(action).unwrap(),
            [
                0, 255, 255, 255, 255, 255, 255, 239, 192, 2, 0x3f, 0, 0, 0, 0x3f, 0x80, 0, 0,
                0x3e, 0x80, 0, 0, 1, 0, 0x81, 1
            ]
        );
        let mut off = encode_use_on(UseOn {
            hand: Hand::Off,
            inside: false,
            world_border: true,
            ..action
        })
        .unwrap();
        assert_eq!(off.remove(0), 1);
        assert_eq!(&off[21..23], &[0, 1]);
        for cursor in [[f32::NAN, 0., 0.], [0., f32::INFINITY, 0.]] {
            assert!(encode_use_on(UseOn { cursor, ..action }).is_err());
        }
        assert!(
            encode_use_on(UseOn {
                position: [0, 2048, 0],
                ..action
            })
            .is_err()
        );
        assert!(
            encode_use_on(UseOn {
                sequence: -1,
                ..action
            })
            .is_err()
        );
    }
    #[test]
    fn native_action_indices_and_signed_position_wire_vectors() {
        let negative = Dig {
            action: Action::Start,
            position: [-1, -64, -2],
            face: Face::North,
            sequence: 128,
        };
        assert_eq!(
            encode(negative).unwrap(),
            [0, 255, 255, 255, 255, 255, 255, 239, 192, 2, 128, 1]
        );
        for (action, id) in [
            (Action::Start, 0),
            (Action::ChangeDirection, 1),
            (Action::Abort, 2),
            (Action::Finish, 3),
        ] {
            let dig = Dig {
                action,
                position: [1, 2, 3],
                face: Face::Up,
                sequence: 1,
            };
            assert_eq!(encode(dig).unwrap(), [id, 0, 0, 0, 64, 0, 0, 48, 2, 1, 1]);
        }
        for position in [[1 << 25, 0, 0], [0, 2048, 0], [0, 0, -(1 << 25) - 1]] {
            assert!(
                encode(Dig {
                    position,
                    ..negative
                })
                .is_err()
            );
        }
        assert!(
            encode(Dig {
                sequence: -1,
                ..negative
            })
            .is_err()
        );
    }
    #[test]
    fn entity_interaction_matches_official_26_3_codec_vectors() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/entity-interact-777.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let expected = case["payload_hex"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks_exact(2)
                .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            let action = EntityUse {
                entity: case["entity"].as_i64().unwrap() as i32,
                hand: if case["hand"] == "MAIN_HAND" {
                    Hand::Main
                } else {
                    Hand::Off
                },
                location: std::array::from_fn(|i| case["location"][i].as_f64().unwrap()),
                secondary: case["secondary"].as_bool().unwrap(),
            };
            assert_eq!(encode_entity_use(action).unwrap(), expected, "{case}");
        }
        for invalid in [f64::NAN, f64::INFINITY, 1e12] {
            assert!(
                encode_entity_use(EntityUse {
                    entity: 42,
                    hand: Hand::Main,
                    location: [0., invalid, 0.],
                    secondary: false
                })
                .is_err()
            );
        }
    }
    #[test]
    fn native_pig_fallback_facts_cover_real_consumers_without_gating_other_items() {
        assert_eq!(target::ITEM_TYPES.len(), target::ITEM_USE_DEFAULTS.len());
        let hint = |name| {
            target::ITEM_USE_DEFAULTS[target::ITEM_TYPES.iter().position(|n| *n == name).unwrap()]
                .pig_interaction_may_consume
        };
        for name in [
            "minecraft:saddle",
            "minecraft:lead",
            "minecraft:name_tag",
            "minecraft:pig_spawn_egg",
        ] {
            assert!(hint(name), "{name}");
        }
        for name in [
            "minecraft:carrot",
            "minecraft:snowball",
            "minecraft:zombie_spawn_egg",
            "minecraft:iron_chestplate",
        ] {
            assert!(!hint(name), "{name}");
        }
        assert_eq!(target::ITEM_COMPONENT_EQUIPPABLE, "minecraft:equippable");
    }
}
