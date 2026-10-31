use super::*;

fn item(operation: i32, slot: i32, display: i32) -> Vec<u8> {
    // Protocol 777: dirt=55, attribute_modifiers=16, attack_damage=3, damage=3.
    let mut bytes = vec![1, 55, 2, 0, 16, 1, 3];
    crate::codec::write_string("fixture:damage", 32767, &mut bytes).unwrap();
    bytes.extend((-2.5f64).to_be_bytes());
    for n in [operation, slot, display] {
        crate::codec::write_varint(n, &mut bytes);
    }
    if display == 2 {
        bytes.extend([8, 0, 4, b'B', b'u', b'f', b'f']); // Anonymous NBT text component.
    }
    bytes.extend([3, 7]); // A second component proves that no old show-tooltip bool is consumed.
    bytes
}

#[test]
fn item_modifiers_preserve_amount_operation_slot_and_display_without_eating_next_component() {
    for (operation, expected_op) in ["add", "add_multiplied_base", "add_multiplied_total"]
        .iter()
        .enumerate()
    {
        for (slot, expected_slot) in [
            "any",
            "main_hand",
            "off_hand",
            "hand",
            "feet",
            "legs",
            "chest",
            "head",
            "armor",
            "body",
            "saddle",
        ]
        .iter()
        .enumerate()
        {
            for (display, expected_display) in ["default", "hidden", "override"].iter().enumerate()
            {
                let bytes = item(operation as i32, slot as i32, display as i32);
                let mut r = Reader(&bytes);
                let stack = stack(&mut r).unwrap().unwrap();
                r.finish().unwrap();
                assert_eq!(stack.added["minecraft:damage"], 7);
                let expected_text = if display == 2 {
                    serde_json::to_value(nbt::Value::String("Buff".into())).unwrap()
                } else {
                    Value::Null
                };
                assert_eq!(
                    stack.added["minecraft:attribute_modifiers"],
                    json!({"modifiers":[{
                        "attribute":"minecraft:attack_damage",
                        "modifier":{"id":"fixture:damage","amount":-2.5,"operation":expected_op},
                        "slot":expected_slot,"display":{"type":expected_display,"component":expected_text}
                    }]})
                );
            }
        }
    }
}

#[test]
fn unknown_display_uses_reference_default_without_consuming_optional_text() {
    for display in [-1, 3, 128, i32::MAX] {
        let bytes = item(0, 1, display);
        let mut r = Reader(&bytes);
        let stack = stack(&mut r).unwrap().unwrap();
        r.finish().unwrap();
        assert_eq!(
            stack.added["minecraft:attribute_modifiers"]["modifiers"][0]["display"],
            json!({"type":"default","component":null})
        );
        assert_eq!(stack.added["minecraft:damage"], 7);
    }
}

#[test]
fn empty_and_removed_modifiers_remain_distinct_patches() {
    let mut r = Reader(&[1, 55, 1, 0, 16, 0]);
    let empty = stack(&mut r).unwrap().unwrap();
    r.finish().unwrap();
    assert_eq!(
        empty.added["minecraft:attribute_modifiers"],
        json!({"modifiers":[]})
    );
    assert!(empty.removed.is_empty());
    let mut r = Reader(&[1, 55, 0, 1, 16]);
    let removed = stack(&mut r).unwrap().unwrap();
    r.finish().unwrap();
    assert!(removed.added.is_empty());
    assert!(removed.removed.contains("minecraft:attribute_modifiers"));
}

#[test]
fn malformed_modifiers_fail_without_returning_partial_items() {
    let bytes = item(2, 10, 2);
    for end in 0..bytes.len() {
        assert!(stack(&mut Reader(&bytes[..end])).is_err(), "prefix {end}");
    }
    for (op, slot) in [(-1, 1), (3, 1), (0, -1), (0, 11)] {
        assert!(stack(&mut Reader(&item(op, slot, 0))).is_err());
    }
    let mut bad_attribute = item(0, 1, 0);
    bad_attribute[6] = 127;
    assert!(stack(&mut Reader(&bad_attribute)).is_err());
    let mut bad_amount = item(0, 1, 0);
    let amount_offset = 7 + 1 + "fixture:damage".len();
    bad_amount[amount_offset..amount_offset + 8].copy_from_slice(&f64::NAN.to_be_bytes());
    assert!(stack(&mut Reader(&bad_amount)).is_err());
    let mut bad_identifier = item(0, 1, 0);
    bad_identifier[8] = b'!';
    assert!(stack(&mut Reader(&bad_identifier)).is_err());
    assert!(stack(&mut Reader(&[1, 55, 1, 0, 16, 255, 255, 127])).is_err());
}
