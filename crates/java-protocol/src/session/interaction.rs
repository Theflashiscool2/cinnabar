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
}
