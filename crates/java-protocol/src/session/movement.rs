//! Version-pinned outbound player movement schemas. Simulation and cadence are domain-owned.
use crate::{ProtocolError, codec};

#[derive(Clone, Debug)]
pub struct Frame {
    pub entity: i32,
    pub position: Option<[f64; 3]>,
    pub rotation: Option<[f32; 2]>,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    /// Seven vanilla input bits, only supplied when intent changes.
    pub input: Option<u8>,
    pub sprinting: Option<bool>,
}

/// Validates the entire frame before serializing any required packet in its ordered batch.
pub(super) fn encode(frame: Frame) -> Result<Vec<(&'static str, Vec<u8>)>, ProtocolError> {
    if frame
        .position
        .is_some_and(|p| !p.into_iter().all(f64::is_finite))
        || frame
            .rotation
            .is_some_and(|p| !p.into_iter().all(f32::is_finite))
        || frame.input.is_some_and(|bits| bits & 128 != 0)
    {
        return Err(ProtocolError::Invalid("native Java movement frame"));
    }
    let mut packets = Vec::with_capacity(4);
    if let Some(bits) = frame.input {
        packets.push(("ServerboundPlayerInputPacket", vec![bits]));
    }
    if let Some(sprinting) = frame.sprinting {
        let mut bytes = Vec::new();
        codec::write_varint(frame.entity, &mut bytes);
        // Pinned PlayerState: LEAVE_BED=0, START_SPRINTING=1, STOP_SPRINTING=2.
        bytes.extend([if sprinting { 1 } else { 2 }, 0]);
        packets.push(("ServerboundPlayerCommandPacket", bytes));
    }
    let mut bytes = Vec::with_capacity(33);
    if let Some(position) = frame.position {
        for v in position {
            bytes.extend(v.to_be_bytes());
        }
    }
    if let Some(rotation) = frame.rotation {
        for v in rotation {
            bytes.extend(v.to_be_bytes());
        }
    }
    bytes.push(u8::from(frame.on_ground) | u8::from(frame.horizontal_collision) << 1);
    let name = match (frame.position.is_some(), frame.rotation.is_some()) {
        (true, true) => "ServerboundMovePlayerPosRotPacket",
        (true, false) => "ServerboundMovePlayerPosPacket",
        (false, true) => "ServerboundMovePlayerRotPacket",
        (false, false) => "ServerboundMovePlayerStatusOnlyPacket",
    };
    packets.push((name, bytes));
    packets.push(("ServerboundClientTickEndPacket", vec![]));
    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_intent_sprint_pose_and_tick_end_match_independent_wire_fields() {
        let packets = encode(Frame {
            entity: 300,
            position: Some([1., -64., 0.5]),
            rotation: Some([90., -45.]),
            on_ground: true,
            horizontal_collision: true,
            input: Some(65),
            sprinting: Some(true),
        })
        .unwrap();
        assert_eq!(packets[0], ("ServerboundPlayerInputPacket", vec![65]));
        assert_eq!(
            packets[1],
            ("ServerboundPlayerCommandPacket", vec![0xac, 2, 1, 0])
        );
        assert_eq!(
            packets[2].1,
            [
                0x3f, 0xf0, 0, 0, 0, 0, 0, 0, 0xc0, 0x50, 0, 0, 0, 0, 0, 0, 0x3f, 0xe0, 0, 0, 0, 0,
                0, 0, 0x42, 0xb4, 0, 0, 0xc2, 0x34, 0, 0, 3
            ]
        );
        assert_eq!(packets[3], ("ServerboundClientTickEndPacket", vec![]));
        for (position, rotation, name, length) in [
            (true, true, "ServerboundMovePlayerPosRotPacket", 33),
            (true, false, "ServerboundMovePlayerPosPacket", 25),
            (false, true, "ServerboundMovePlayerRotPacket", 9),
            (false, false, "ServerboundMovePlayerStatusOnlyPacket", 1),
        ] {
            let frame = Frame {
                entity: 1,
                position: position.then_some([0.; 3]),
                rotation: rotation.then_some([0.; 2]),
                on_ground: false,
                horizontal_collision: false,
                input: None,
                sprinting: None,
            };
            let p = encode(frame.clone()).unwrap();
            assert_eq!((p[0].0, p[0].1.len()), (name, length));
            assert!(
                encode(Frame {
                    input: Some(128),
                    ..frame.clone()
                })
                .is_err()
            );
            assert!(
                encode(Frame {
                    position: Some([f64::NAN; 3]),
                    ..frame
                })
                .is_err()
            );
        }
    }
}
