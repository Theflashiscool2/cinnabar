use super::*;

#[test]
fn packed_velocity_matches_independent_pinned_vanilla_vectors() {
    // Exported by LpVec3.write/read from the hash-pinned vanilla server, not our encoder.
    let cases: &[(&[u8], [u64; 3])] = &[
        (&[0], [0; 3]),
        (
            &[0xf1, 0xff, 0, 0, 0xff, 0xff],
            [0x3ff0000000000000, 0, 0xbff0000000000000],
        ),
        (
            &[0xdc, 7, 0xfe, 0x3a, 0xf1, 0x66, 2],
            [0x3fbf807e01f80800, 0xbfdd3074c1d30750, 0x401f8f7e3df8f7e4],
        ),
        (
            &[7, 0, 0xff, 0xfc, 0xff, 0xfc, 0xff, 0xff, 0xff, 0xff, 15],
            [0xc20ffffffff80000, 0, 0x420ffffffff80000],
        ),
    ];
    for &(bytes, expected) in cases {
        let mut r = Reader(bytes);
        assert_eq!(velocity(&mut r).unwrap().map(f64::to_bits), expected);
        r.finish().unwrap();
        for end in 0..bytes.len() {
            assert!(velocity(&mut Reader(&bytes[..end])).is_err());
        }
    }
    assert!(velocity(&mut Reader(&[7, 0, 0, 0, 0, 0, 255, 255, 255, 255, 31])).is_err());
}

#[test]
fn abilities_and_rotation_have_version_specific_field_order() {
    let mut r = Reader(&[13, 0x3d, 0x4c, 0xcc, 0xcd, 0x3d, 0xcc, 0xcc, 0xcd]);
    let Some(Packet::Abilities(a)) = decode("ClientboundPlayerAbilitiesPacket", &mut r).unwrap()
    else {
        panic!()
    };
    assert!(a.invulnerable && a.may_fly && a.creative && !a.flying);
    assert_eq!([a.flying_speed, a.walking_speed], [0.05, 0.1]);
    r.finish().unwrap();
    let mut r = Reader(&[0x42, 0xb4, 0, 0, 1, 0xc2, 0x34, 0, 0, 0]);
    let Some(Packet::Rotation {
        yaw,
        relative_yaw,
        pitch,
        relative_pitch,
    }) = decode("ClientboundPlayerRotationPacket", &mut r).unwrap()
    else {
        panic!()
    };
    assert_eq!([yaw, pitch], [90., -45.]);
    assert!(relative_yaw && !relative_pitch);
    r.finish().unwrap();
    assert!(decode("ClientboundPlayerRotationPacket", &mut Reader(&[0; 9])).is_err());
    assert!(decode("ClientboundPlayerAbilitiesPacket", &mut Reader(&[16; 9])).is_err());
}

#[test]
fn attributes_resolve_pinned_registry_and_reject_invalid_modifiers() {
    // Official registries report: movement_speed is ID26. Base .1; one sprint modifier,
    // ADD_MULTIPLIED_TOTAL, with the vanilla float .3 promoted to double.
    let mut bytes = vec![
        7, 1, 26, 0x3f, 0xb9, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9a, 1, 9,
    ];
    bytes.extend(b"sprinting");
    bytes.extend([0x3f, 0xd3, 0x33, 0x33, 0x40, 0, 0, 0, 2]);
    let mut r = Reader(&bytes);
    let Some(Packet::Attributes { entity, values }) =
        decode("ClientboundUpdateAttributesPacket", &mut r).unwrap()
    else {
        panic!()
    };
    r.finish().unwrap();
    assert_eq!(entity, 7);
    assert_eq!(values[0].name, "minecraft:movement_speed");
    assert_eq!(values[0].base, 0.1);
    assert_eq!(values[0].modifiers[0].name, "minecraft:sprinting");
    assert_eq!(values[0].modifiers[0].amount, f64::from(0.3_f32));
    for end in 0..bytes.len() {
        assert!(
            decode(
                "ClientboundUpdateAttributesPacket",
                &mut Reader(&bytes[..end])
            )
            .is_err()
        );
    }
    *bytes.last_mut().unwrap() = 3;
    assert!(decode("ClientboundUpdateAttributesPacket", &mut Reader(&bytes)).is_err());
    assert!(
        decode(
            "ClientboundUpdateAttributesPacket",
            &mut Reader(&[7, 1, 127])
        )
        .is_err()
    );
}
