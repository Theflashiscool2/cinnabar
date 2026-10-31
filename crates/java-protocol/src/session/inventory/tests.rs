use super::*;

#[test]
fn independent_native_item_and_container_vectors_preserve_patch_semantics() {
    // Official item registry: dirt=55, iron_pickaxe=1047. Maintained component schema: damage=3.
    let bytes = [0, 128, 1, 2, 64, 55, 0, 0, 1, 0x97, 8, 1, 0, 3, 7, 0];
    let mut r = Reader(&bytes);
    let Packet::Content {
        container,
        state,
        slots,
        carried,
    } = decode("ClientboundContainerSetContentPacket", &mut r)
        .unwrap()
        .unwrap()
    else {
        panic!()
    };
    r.finish().unwrap();
    assert_eq!((container, state), (0, 128));
    let dirt = slots[0].as_ref().unwrap();
    assert_eq!((dirt.item, dirt.count), (55, 64));
    assert_eq!(target::ITEM_TYPES[dirt.item as usize], "minecraft:dirt");
    assert!(dirt.added.is_empty() && dirt.removed.is_empty());
    assert_eq!(slots[1].as_ref().unwrap().added["minecraft:damage"], 7);
    assert!(carried.is_none());
    let mut r = Reader(&[1, 55, 0, 1, 3]);
    assert!(
        stack(&mut r)
            .unwrap()
            .unwrap()
            .removed
            .contains("minecraft:damage")
    );
    r.finish().unwrap();
    let mut r = Reader(&[255, 255, 255, 255, 15, 123]);
    assert!(stack(&mut r).unwrap().is_none());
    assert_eq!(r.0, [123]);
}
#[test]
fn malformed_components_never_publish_a_partial_snapshot() {
    let bytes = [0, 1, 2, 64, 55, 0, 0, 1, 0x97, 8, 1, 0, 3, 7, 0];
    for end in 0..bytes.len() {
        assert!(
            decode(
                "ClientboundContainerSetContentPacket",
                &mut Reader(&bytes[..end])
            )
            .is_err(),
            "prefix {end}"
        );
    }
    for bytes in [
        &[1, 55, 2, 0, 3, 1, 3, 2][..],
        &[1, 55, 1, 1, 3, 1, 3],
        &[1, 55, 1, 0, 255, 127],
        &[1, 255, 127, 0, 0],
    ] {
        assert!(stack(&mut Reader(bytes)).is_err());
    }
    assert!(matches!(
        stack(&mut Reader(&[1, 55, 1, 0, 24, 0])),
        Err(Error::Truncated)
    ));
}
#[test]
fn signed_container_fields_and_titles_use_the_target_schemas() {
    let mut r = Reader(&[7, 255, 255, 128, 0]);
    let Packet::Property {
        container,
        property,
        value,
    } = decode("ClientboundContainerSetDataPacket", &mut r)
        .unwrap()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!((container, property, value), (7, -1, -32768));
    r.finish().unwrap();
    let mut r = Reader(&[7, 2, 8, 0, 5, b'C', b'h', b'e', b's', b't']);
    let Packet::Open {
        container,
        kind,
        title,
    } = decode("ClientboundOpenScreenPacket", &mut r)
        .unwrap()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(container, 7);
    assert_eq!(kind, "GENERIC_9X3");
    assert_eq!(title, nbt::Value::String("Chest".into()));
    r.finish().unwrap();
}

#[test]
fn tool_rule_options_and_float_order_match_independent_wire_values() {
    let bytes = [
        1, 55, 1, 0, 28, 1, 2, 5, 1, 0x40, 0, 0, 0, 1, 1, 0x3f, 0x80, 0, 0, 1, 0,
    ];
    let mut r = Reader(&bytes);
    let item = stack(&mut r).unwrap().unwrap();
    r.finish().unwrap();
    assert_eq!(
        item.added["minecraft:tool"],
        serde_json::json!({
            "rules":[{"blocks":{"ids":[5]},"speed":2.,"correct_for_drops":true}],
            "default_mining_speed":1.,"damage_per_block":1,"can_destroy_blocks_in_creative":false
        })
    );
    for end in 1..bytes.len() {
        assert!(stack(&mut Reader(&bytes[..end])).is_err());
    }
    let mut bytes = vec![0, 0, 0xc0, 0x84, 0x3d]; // A million empty slots expand beyond the admission budget.
    bytes.resize(bytes.len() + 1_000_001, 0);
    assert!(decode("ClientboundContainerSetContentPacket", &mut Reader(&bytes)).is_err());
}
