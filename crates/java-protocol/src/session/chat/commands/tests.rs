use super::*;
fn fixture() -> Tree {
    let hex = include_str!("../../../../data/command-tree-fixture.hex").trim();
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    let mut reader = Reader(&bytes);
    let tree = Tree::decode(&mut reader).unwrap();
    reader.finish().unwrap();
    tree
}
#[test]
fn native_command_tree_message_ranges_aliases_and_root_redirects() {
    let tree = fixture();
    assert_eq!(tree.counts(), (8, 1));
    for (text, expected) in [
        ("msg Bob hello 夜", Some("hello 夜")),
        ("w Bob hi there", Some("hi there")),
        ("execute run msg Bob nested", None),
        ("msg Bob ", None),
        ("msg Bob hello @a[bad", Some("hello @a[bad")),
        ("spawn", None),
        ("unknown words", None),
    ] {
        let expected = expected
            .map(|s| vec![("message".into(), s.into())])
            .unwrap_or_default();
        assert_eq!(tree.arguments(text, 0).unwrap(), expected, "{text}");
    }
    assert!(tree.arguments("msg Bob hello @a[bad", 4).is_err());
}
#[test]
fn network_tree_truncations_and_invalid_references_cannot_publish() {
    let hex = include_str!("../../../../data/command-tree-fixture.hex").trim();
    let mut bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    for end in 0..bytes.len() {
        assert!(Tree::decode(&mut Reader(&bytes[..end])).is_err(), "{end}");
    }
    let mut cycle = bytes.clone();
    cycle[3] = 0;
    assert!(Tree::decode(&mut Reader(&cycle)).is_err());
    *bytes.last_mut().unwrap() = 99;
    assert!(Tree::decode(&mut Reader(&bytes)).is_err());
}
#[test]
fn native_signed_command_without_certificate_has_exact_fields_and_atomic_failures() {
    let mut state = super::super::State::default();
    state.commands = Some(fixture());
    let packet = state
        .outgoing_command("msg Bob hello", 1700000000123, 42)
        .unwrap();
    assert_eq!(packet.packet, "ServerboundChatCommandSignedPacket");
    let expected = "0d6d736720426f622068656c6c6f0000018bcfe5687b000000000000002a000000000001";
    let expected: Vec<_> = expected
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    assert_eq!(packet.payload, expected);
    state.secure = true;
    state.window.observe([5; 256], true);
    assert!(
        state
            .outgoing_command("msg Bob blocked", 1700000000123, 42)
            .is_err()
    );
    assert_eq!(state.window.offset(), 1);
    assert_eq!(
        state
            .outgoing_command("spawn", 1700000000123, 42)
            .unwrap()
            .packet,
        "ServerboundChatCommandPacket"
    );
}
#[test]
fn command_tree_lifetimes_follow_native_play_and_configuration_handlers() {
    let mut state = super::super::State::default();
    state.commands = Some(fixture());
    state.command_permission = 4;
    state.command_entity = Some(9);
    state.world_login(false, false, 0).unwrap();
    assert!(state.commands.is_some());
    assert_eq!(state.command_permission, 0);
    assert!(state.command_entity.is_none());
    state.reconfigure().unwrap();
    assert!(state.commands.is_none());
    for (byte, expected) in [
        (23, None),
        (24, Some(0)),
        (26, Some(2)),
        (28, Some(4)),
        (29, None),
    ] {
        assert_eq!(super::super::permission_level(byte), expected);
    }
}

#[test]
fn native_all_parser_properties_and_custom_provider_fields_keep_alignment() {
    let hex = include_str!("../../../../data/command-properties-fixture.hex").trim();
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    let mut reader = Reader(&bytes);
    let tree = Tree::decode(&mut reader).unwrap();
    reader.finish().unwrap();
    assert_eq!(tree.counts(), (63, 1));
    let long = &tree.nodes[5];
    assert!(
        matches!(&long.kind,Kind::Argument {properties:Properties::Number {lower:Some(low),upper:Some(high)},..} if low == &(-9007199254740993i64).to_be_bytes() && high == &(9007199254740993i64).to_be_bytes())
    );
}
