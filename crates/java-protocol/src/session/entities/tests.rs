use super::*;

fn packet(name: &str, bytes: &[u8]) -> Result<Packet, Error> {
    let mut r = Reader(bytes);
    let packet = decode(name, &mut r)?.expect("identified fixture packet");
    r.finish()?;
    Ok(packet)
}

#[test]
fn spawn_has_packed_velocity_before_signed_angles_and_native_registry_identity() {
    // ID 300, UUID 00..0f, official target report pig=101, position (1,-64,.5),
    // LpVec3 zero, pitch -90, yaw 90, head -180, object data 127.
    let bytes = [
        0xac, 2, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 101, 0x3f, 0xf0, 0, 0, 0, 0,
        0, 0, 0xc0, 0x50, 0, 0, 0, 0, 0, 0, 0x3f, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0xc0, 0x40, 0x80,
        0x7f,
    ];
    let Packet::Spawn(spawn) = packet("ClientboundAddEntityPacket", &bytes).unwrap() else {
        panic!()
    };
    assert_eq!(spawn.id, 300);
    assert_eq!(spawn.uuid, std::array::from_fn(|i| i as u8));
    assert_eq!(spawn.kind, "minecraft:pig");
    assert_eq!(spawn.position, [1., -64., 0.5]);
    assert_eq!(spawn.velocity, [0.; 3]);
    assert_eq!([spawn.pitch, spawn.yaw, spawn.head_yaw], [-90., 90., -180.]);
    assert_eq!(spawn.data, 127);
    for end in 0..bytes.len() {
        assert!(packet("ClientboundAddEntityPacket", &bytes[..end]).is_err());
    }
    let mut unknown = bytes.to_vec();
    unknown.splice(18..19, [0xa1, 1]); // The target registry has IDs 0..160.
    assert!(packet("ClientboundAddEntityPacket", &unknown).is_err());
}

#[test]
fn stepped_deltas_preserve_signed_short_units_and_tick_order() {
    // Ground bit + two steps; second endpoint's rotation follows every step.
    let bytes = [
        42, 5, 2, 0x10, 0, 0xff, 0xff, 0x80, 0, 4, 0, 1, 0, 0, 0x7f, 0xff, 0x40, 0xc0,
    ];
    let Packet::Move {
        id,
        delta,
        rotation,
        on_ground,
    } = packet("ClientboundMoveEntityPosRotPacket", &bytes).unwrap()
    else {
        panic!()
    };
    assert_eq!(id, 42);
    assert!(on_ground);
    assert_eq!(rotation, Some([90., -90.]));
    assert_eq!(
        delta,
        Delta::Stepped(vec![
            DeltaStep {
                ticks: 2,
                delta: [4096, -1, -32768]
            },
            DeltaStep {
                ticks: 4,
                delta: [1, 0, 32767]
            },
        ])
    );
    assert!(packet("ClientboundMoveEntityPosPacket", &[42, 4, 1, 0, 0]).is_err());
    assert!(
        packet(
            "ClientboundMoveEntityPosPacket",
            &[42, 0xff, 0xff, 0xff, 0xff, 15]
        )
        .is_err()
    );
    let Packet::Move { delta, .. } = packet(
        "ClientboundMoveEntityPosPacket",
        &[42, 0, 0x80, 0, 0, 0, 0, 1],
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(delta, Delta::Linear([-32768, 0, 1]));
}

#[test]
fn rotation_ground_order_and_lifecycle_lists_are_not_previous_version_layouts() {
    let Packet::Rotate {
        id,
        rotation,
        on_ground,
    } = packet("ClientboundMoveEntityRotPacket", &[42, 1, 0x80, 0x40]).unwrap()
    else {
        panic!()
    };
    assert_eq!(id, 42);
    assert_eq!(rotation, [-180., 90.]);
    assert!(on_ground);
    assert!(packet("ClientboundMoveEntityRotPacket", &[42, 2, 0, 0]).is_err());
    let Packet::Remove(ids) =
        packet("ClientboundRemoveEntitiesPacket", &[3, 1, 0xac, 2, 0]).unwrap()
    else {
        panic!()
    };
    assert_eq!(ids, [1, 300, 0]);
    assert!(packet("ClientboundRemoveEntitiesPacket", &[3, 1, 2]).is_err());
    assert!(packet("ClientboundRemoveEntitiesPacket", &[0, 1]).is_err());
    let Packet::Passengers {
        vehicle,
        passengers,
    } = packet("ClientboundSetPassengersPacket", &[42, 2, 0xac, 2, 1]).unwrap()
    else {
        panic!()
    };
    assert_eq!(vehicle, 42);
    assert_eq!(passengers, [300, 1]);
}

#[test]
fn position_path_tags_counts_and_nonfinite_coordinates_are_checked() {
    assert!(packet("ClientboundEntityPositionSyncPacket", &[42, 2]).is_err());
    assert!(packet("ClientboundEntityPositionSyncPacket", &[42, 1, 2]).is_err());
    assert!(packet("ClientboundEntityPositionSyncPacket", &[42, 1, 0]).is_err());
    // Linear position (0,0,0), yaw 90, pitch 0, airborne.
    let mut bytes = vec![42, 0];
    bytes.extend([0; 24]);
    bytes.extend([0x42, 0xb4, 0, 0, 0, 0, 0, 0, 0]);
    let Packet::Position {
        path,
        rotation,
        on_ground,
        ..
    } = packet("ClientboundEntityPositionSyncPacket", &bytes).unwrap()
    else {
        panic!()
    };
    assert_eq!(path, Position::Linear([0.; 3]));
    assert_eq!(rotation, [90., 0.]);
    assert!(!on_ground);
    bytes[2..10].copy_from_slice(&[0x7f, 0xf0, 0, 0, 0, 0, 0, 0]);
    assert!(packet("ClientboundEntityPositionSyncPacket", &bytes).is_err());
}
