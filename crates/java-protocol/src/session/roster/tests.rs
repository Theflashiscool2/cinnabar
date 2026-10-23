use super::*;

fn packet(name: &str, bytes: &[u8]) -> Result<Packet, Error> {
    let mut reader = Reader(bytes);
    let packet = decode(name, &mut reader)?.expect("fixture packet");
    reader.finish()?;
    Ok(packet)
}

#[test]
fn independent_all_action_vector_preserves_nullable_fields_and_order() {
    // Fixed eight-bit action mask; one UUID 00..0f; signed textures property;
    // cleared chat, spectator, listed, latency 300, NBT string Hi, order -1, hidden hat.
    let bytes = [
        255, 1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 4, b'P', b'e', b'e', b'r', 1,
        8, b't', b'e', b'x', b't', b'u', b'r', b'e', b's', 3, b'a', b'b', b'c', 1, 3, b's', b'i',
        b'g', 0, 3, 1, 0xac, 2, 1, 8, 0, 2, b'H', b'i', 255, 255, 255, 255, 15, 0,
    ];
    let Packet::Update(updates) = packet("ClientboundPlayerInfoUpdatePacket", &bytes).unwrap()
    else {
        panic!()
    };
    let update = &updates[0];
    assert_eq!(update.uuid, std::array::from_fn(|n| n as u8));
    let profile = update.profile.as_ref().unwrap();
    assert_eq!(profile.name, "Peer");
    assert_eq!(profile.properties[0].name, "textures");
    assert_eq!(profile.properties[0].value, "abc");
    assert_eq!(profile.properties[0].signature.as_deref(), Some("sig"));
    assert!(matches!(update.chat, Some(None)));
    assert_eq!(update.mode, Some(GameMode::Spectator));
    assert_eq!(update.listed, Some(true));
    assert_eq!(update.latency, Some(300));
    assert_eq!(
        update.display_name,
        Some(Some(nbt::Value::String("Hi".into())))
    );
    assert_eq!(update.order, Some(-1));
    assert_eq!(update.show_hat, Some(false));
    for end in 0..bytes.len() {
        assert!(
            packet("ClientboundPlayerInfoUpdatePacket", &bytes[..end]).is_err(),
            "end={end}"
        );
    }
}

#[test]
fn sparse_updates_and_uuid_removal_do_not_invent_profiles() {
    let bytes = [
        0xa8, 1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 0, 1,
    ];
    let Packet::Update(updates) = packet("ClientboundPlayerInfoUpdatePacket", &bytes).unwrap()
    else {
        panic!()
    };
    let update = &updates[0];
    assert!(update.profile.is_none() && update.chat.is_none() && update.mode.is_none());
    assert_eq!(update.display_name, Some(None));
    assert_eq!(update.listed, Some(false));
    assert_eq!(update.show_hat, Some(true));
    let Packet::Remove(ids) = packet("ClientboundPlayerInfoRemovePacket", &bytes[1..18]).unwrap()
    else {
        panic!()
    };
    assert_eq!(ids, [update.uuid]);
    assert!(packet("ClientboundPlayerInfoRemovePacket", &[2, 0]).is_err());
    let mut malformed = bytes.to_vec();
    malformed[18] = 2;
    assert!(packet("ClientboundPlayerInfoUpdatePacket", &malformed).is_err());
    assert!(
        packet(
            "ClientboundPlayerInfoUpdatePacket",
            &[1, 255, 255, 255, 255, 15]
        )
        .is_err()
    );
}

#[test]
fn chat_key_encoding_is_checked_without_claiming_signature_trust() {
    use rand::{SeedableRng, rngs::StdRng};
    use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
    let private = RsaPrivateKey::new(&mut StdRng::seed_from_u64(17), 1024).unwrap();
    let public = private.to_public_key().to_public_key_der().unwrap();
    let mut bytes = vec![2, 1];
    bytes.extend([0; 16]); // Profile UUID.
    bytes.push(1);
    bytes.extend([1; 16]); // Chat-session UUID.
    bytes.extend(42i64.to_be_bytes());
    crate::wire::bytes(public.as_ref(), &mut bytes).unwrap();
    crate::wire::bytes(&[1, 2, 3], &mut bytes).unwrap(); // Unverified service signature.
    let Packet::Update(updates) = packet("ClientboundPlayerInfoUpdatePacket", &bytes).unwrap()
    else {
        panic!()
    };
    let chat = updates[0].chat.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(chat.expires_at, 42);
    assert_eq!(chat.public_key, public.as_ref());
    assert_eq!(chat.signature, [1, 2, 3]);
    let key_start = bytes.len() - public.as_ref().len() - 4;
    bytes[key_start] = 0;
    assert!(packet("ClientboundPlayerInfoUpdatePacket", &bytes).is_err());
}
