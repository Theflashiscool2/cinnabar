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
/// The caller owns sequence allocation and edition/session selection.
pub fn encode(dig: Dig) -> Result<Vec<u8>, Error> {
    let [x, y, z] = dig.position;
    if !(-(1 << 25)..(1 << 25)).contains(&x)
        || !(-(1 << 25)..(1 << 25)).contains(&z)
        || !(-2048..2048).contains(&y)
        || dig.sequence < 0
    {
        return Err(Error::Invalid("Java block action coordinate/sequence"));
    }
    let action = match dig.action {
        Action::Start => target::PLAYER_ACTION_START_DIGGING,
        Action::ChangeDirection => target::PLAYER_ACTION_CHANGE_DIGGING_DIRECTION,
        Action::Abort => target::PLAYER_ACTION_CANCEL_DIGGING,
        Action::Finish => target::PLAYER_ACTION_FINISH_DIGGING,
    };
    let face = match dig.face {
        Face::Down => target::BLOCK_FACE_DOWN,
        Face::Up => target::BLOCK_FACE_UP,
        Face::North => target::BLOCK_FACE_NORTH,
        Face::South => target::BLOCK_FACE_SOUTH,
        Face::West => target::BLOCK_FACE_WEST,
        Face::East => target::BLOCK_FACE_EAST,
    };
    let mut bytes = Vec::with_capacity(15);
    write_varint(i32::from(action), &mut bytes);
    let position =
        ((x as u64 & 0x3ff_ffff) << 38) | ((z as u64 & 0x3ff_ffff) << 12) | (y as u64 & 0xfff);
    bytes.extend(position.to_be_bytes());
    bytes.push(face);
    write_varint(dig.sequence, &mut bytes);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
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
