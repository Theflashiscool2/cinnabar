//! Synthetic authority fixtures; no Mojang access or vanilla live qualification is implied.
use super::*;
use rand::SeedableRng;
use rsa::{Pkcs1v15Sign, RsaPrivateKey, pkcs8::EncodePublicKey};
use sha2::Digest;
fn fixture(expires: i64) -> (crate::auth::chat::Certificate, security::Roots) {
    let mut random = rand::rngs::StdRng::seed_from_u64(37812);
    let root = RsaPrivateKey::new(&mut random, 1024).unwrap();
    let private = RsaPrivateKey::new(&mut random, 2048).unwrap();
    let public = private
        .to_public_key()
        .to_public_key_der()
        .unwrap()
        .as_bytes()
        .to_vec();
    let owner = [7; 16];
    let mut payload = owner.to_vec();
    payload.extend(expires.to_be_bytes());
    payload.extend(&public);
    let signature = root
        .sign(
            Pkcs1v15Sign::new::<sha1::Sha1>(),
            &sha1::Sha1::digest(payload),
        )
        .unwrap();
    let roots = security::Roots::from_service_keys([root
        .to_public_key()
        .to_public_key_der()
        .unwrap()
        .as_bytes()
        .to_vec()])
    .unwrap();
    (
        crate::auth::chat::Certificate {
            owner,
            expires_at: expires,
            refreshed_after: expires,
            public_der: public,
            signature,
            private,
        },
        roots,
    )
}
fn decode_hex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
fn incoming(global: u8, index: u8, signature: &window::Signature, text: &str) -> Vec<u8> {
    let mut out = vec![global];
    out.extend([7; 16]);
    out.push(index);
    out.push(1);
    out.extend(signature);
    crate::codec::write_string(text, PLAYER_MAX_UTF16, &mut out).unwrap();
    out.extend(1700000000123i64.to_be_bytes());
    out.extend(42i64.to_be_bytes());
    out.extend([0, 0, 0]);
    // Official ProbeChat serialized direct ChatType.Bound (sender Peer, no target).
    out.extend(decode_hex("000e636861742e747970652e746578740200020a000e636861742e747970652e746578740200020a000800045065657200"));
    out
}
fn read_outgoing(bytes: &[u8]) -> (String, i64, i64, window::Signature, Vec<u8>) {
    let mut r = Reader(bytes);
    let text = crate::codec::read_string(&mut r.0, PLAYER_MAX_UTF16)
        .unwrap()
        .to_owned();
    let timestamp = r.i64().unwrap();
    let salt = r.i64().unwrap();
    assert!(r.bool().unwrap());
    let signature = r.take(window::SIGNATURE_BYTES).unwrap().try_into().unwrap();
    (text, timestamp, salt, signature, r.0.to_vec())
}
#[test]
fn signed_send_verified_receive_seen_update_and_poisoned_chain() {
    let (certificate, roots) = fixture(4_070_908_800_000);
    let public = certificate.private.to_public_key();
    let cert = super::super::roster::ChatSession {
        id: [0; 16],
        expires_at: certificate.expires_at,
        public_key: certificate.public_der.clone(),
        signature: certificate.signature.clone(),
    };
    let mut state = State::default();
    state.secure = true;
    state.online = true;
    assert!(!state.can_send());
    let announcement = state
        .install(
            crate::auth::chat::Material {
                roots,
                certificate: Some(certificate),
            },
            1_700_000_000_123,
        )
        .unwrap()
        .unwrap();
    assert!(state.can_send());
    let id: [u8; 16] = announcement[..16].try_into().unwrap();
    let mut r = Reader(&announcement[16..]);
    assert_eq!(r.i64().unwrap(), cert.expires_at);
    let n = r.var().unwrap() as usize;
    assert_eq!(r.take(n).unwrap(), cert.public_key);
    let n = r.var().unwrap() as usize;
    assert_eq!(r.take(n).unwrap(), cert.signature);
    r.finish().unwrap();
    let bytes = state.outgoing("Hello", 1_700_000_000_123, 42).unwrap();
    let (text, timestamp, salt, signature, seen) = read_outgoing(&bytes);
    assert_eq!(seen, [0, 0, 0, 0, 1]);
    security::Body {
        sender: [7; 16],
        session: id,
        index: 0,
        text: &text,
        timestamp_millis: timestamp,
        salt,
        last_seen: &[],
    }
    .verify(&public, &signature)
    .unwrap();
    let packet = super::super::roster::Packet::Update(vec![super::super::roster::Update {
        uuid: [7; 16],
        chat: Some(Some(super::super::roster::ChatSession {
            id,
            ..cert.clone()
        })),
        ..Default::default()
    }]);
    state.observe_roster(&packet);
    let c = super::super::facts::Configuration::default();
    let bytes = incoming(0, 0, &signature, "Hello");
    let mut r = Reader(&bytes);
    let message = state
        .receive("ClientboundPlayerChatPacket", &mut r, &c)
        .unwrap()
        .unwrap();
    r.finish().unwrap();
    assert!(matches!(message.content, nbt::Value::Compound(_)));
    let reply = state.outgoing("Reply", 1_700_000_000_123, 77).unwrap();
    let (text, timestamp, salt, sig, seen) = read_outgoing(&reply);
    assert_eq!(&seen[..4], &[1, 0, 0, 8]);
    assert_ne!(seen[4], 0);
    security::Body {
        sender: [7; 16],
        session: id,
        index: 1,
        text: &text,
        timestamp_millis: timestamp,
        salt,
        last_seen: &[signature],
    }
    .verify(&public, &sig)
    .unwrap();
    let bad = incoming(1, 2, &[0; window::SIGNATURE_BYTES], "forged");
    let reply = state
        .receive("ClientboundPlayerChatPacket", &mut Reader(&bad), &c)
        .unwrap()
        .unwrap();
    assert!(
        matches!(reply.content,nbt::Value::String(ref text) if text.contains("could not be verified"))
    );
    // Even a later cryptographically correct message cannot revive the broken chain.
    let body = security::Body {
        sender: [7; 16],
        session: id,
        index: 3,
        text: "later",
        timestamp_millis: 1_700_000_000_123,
        salt: 42,
        last_seen: &[],
    };
    let sig: window::Signature = state
        .local
        .as_ref()
        .unwrap()
        .certificate
        .private
        .sign(
            Pkcs1v15Sign::new::<sha2::Sha256>(),
            &sha2::Sha256::digest(body.payload().unwrap()),
        )
        .unwrap()
        .try_into()
        .unwrap();
    let bytes = incoming(2, 3, &sig, "later");
    let reply = state
        .receive("ClientboundPlayerChatPacket", &mut Reader(&bytes), &c)
        .unwrap()
        .unwrap();
    assert!(
        matches!(reply.content,nbt::Value::String(ref text) if text.contains("could not be verified"))
    );
    assert!(state.reconfigure().unwrap().is_none());
    assert!(state.local.is_none());
    assert!(state.cached_certificate.is_some());
    let new = state
        .world_login(true, true, 1_700_000_000_123)
        .unwrap()
        .unwrap();
    assert_ne!(&new[..16], &id);
    assert!(state.roots.is_some());
    assert_eq!(state.next_index, 0);
    assert!(state.local.is_some());
}
#[test]
fn rejected_local_signing_does_not_consume_last_seen_state() {
    let (certificate, roots) = fixture(100);
    let mut state = State::default();
    state.online = true;
    state
        .install(
            crate::auth::chat::Material {
                roots,
                certificate: Some(certificate),
            },
            99,
        )
        .unwrap();
    let mut signature = [0; window::SIGNATURE_BYTES];
    signature[0] = 1;
    state.window.observe(signature, true);
    assert!(state.outgoing("expired", 101, 42).is_err());
    assert_eq!(
        state.window.update(),
        [1, 0, 0, 8, 255],
        "no unseen update was sent on the failed signing path"
    );
}

#[test]
fn renewing_the_same_key_preserves_chain_and_world_mode_controls_signing() {
    let (certificate, roots) = fixture(4_070_908_800_000);
    let public = certificate.private.to_public_key();
    let mut state = State::default();
    state.online = true;
    let announcement = state
        .install(
            crate::auth::chat::Material {
                roots,
                certificate: Some(certificate),
            },
            1_700_000_000_123,
        )
        .unwrap()
        .unwrap();
    let id: [u8; 16] = announcement[..16].try_into().unwrap();
    state.outgoing("first", 1_700_000_000_123, 1).unwrap();
    let (certificate, roots) = fixture(4_070_908_800_000);
    assert!(
        state
            .install(
                crate::auth::chat::Material {
                    roots,
                    certificate: Some(certificate)
                },
                1_700_000_000_123
            )
            .unwrap()
            .is_none()
    );
    let (text, timestamp, salt, signature, seen) =
        read_outgoing(&state.outgoing("second", 1_700_000_000_123, 2).unwrap());
    assert_eq!(seen, [0, 0, 0, 0, 1]);
    security::Body {
        sender: [7; 16],
        session: id,
        index: 1,
        text: &text,
        timestamp_millis: timestamp,
        salt,
        last_seen: &[],
    }
    .verify(&public, &signature)
    .unwrap();
    assert!(!state.needs_preparation(1_700_000_000_123));
    assert!(state.needs_preparation(4_070_908_800_001));
    assert_eq!(
        crate::auth::chat::MINIMUM_REFRESH_INTERVAL_MILLIS,
        3_600_000
    );
    assert!(
        state
            .world_login(false, false, 1_700_000_000_123)
            .unwrap()
            .is_none()
    );
    assert!(state.local.is_none());
    assert!(state.cached_certificate.is_some());
    let bytes = state
        .outgoing("offline world", 1_700_000_000_123, 3)
        .unwrap();
    let mut r = Reader(&bytes);
    crate::codec::read_string(&mut r.0, PLAYER_MAX_UTF16).unwrap();
    r.i64().unwrap();
    r.i64().unwrap();
    assert!(!r.bool().unwrap());
    let new = state
        .world_login(true, true, 1_700_000_000_123)
        .unwrap()
        .unwrap();
    assert_ne!(&new[..16], &id);
    let id = new[..16].try_into().unwrap();
    let (text, timestamp, salt, signature, seen) =
        read_outgoing(&state.outgoing("new world", 1_700_000_000_123, 4).unwrap());
    assert_eq!(seen, [0, 0, 0, 0, 1]);
    security::Body {
        sender: [7; 16],
        session: id,
        index: 0,
        text: &text,
        timestamp_millis: timestamp,
        salt,
        last_seen: &[],
    }
    .verify(&public, &signature)
    .unwrap();
}
#[test]
fn secure_world_does_not_display_unsigned_messages_without_a_trusted_session() {
    let mut state = State::default();
    state.secure = true;
    let mut bytes = incoming(0, 0, &[0; window::SIGNATURE_BYTES], "unattributed");
    bytes[18] = 0;
    bytes.drain(19..19 + window::SIGNATURE_BYTES);
    let message = state
        .receive(
            "ClientboundPlayerChatPacket",
            &mut Reader(&bytes),
            &super::super::facts::Configuration::default(),
        )
        .unwrap()
        .unwrap();
    assert!(
        matches!(message.content,nbt::Value::String(ref text) if text.contains("did not match") && !text.contains("unattributed"))
    );
    assert!(
        state
            .outgoing("no certificate", 1_700_000_000_123, 5)
            .is_err()
    );
}
#[test]
fn remote_certificate_grace_uses_native_eight_hours() {
    let expiry = 4_070_908_800_000;
    let (certificate, roots) = fixture(expiry);
    let mut state = State::default();
    state.online = true;
    let cert = super::super::roster::ChatSession {
        id: [0; 16],
        expires_at: expiry,
        public_key: certificate.public_der.clone(),
        signature: certificate.signature.clone(),
    };
    let announcement = state
        .install(
            crate::auth::chat::Material {
                roots,
                certificate: Some(certificate),
            },
            1_700_000_000_123,
        )
        .unwrap()
        .unwrap();
    let id = announcement[..16].try_into().unwrap();
    state.observe_roster(&super::super::roster::Packet::Update(vec![
        super::super::roster::Update {
            uuid: [7; 16],
            chat: Some(Some(super::super::roster::ChatSession { id, ..cert })),
            ..Default::default()
        },
    ]));
    let (_, _, _, sig, _) = read_outgoing(&state.outgoing("Hello", 1_700_000_000_123, 42).unwrap());
    let bytes = incoming(0, 0, &sig, "Hello");
    let message = player::player(&mut Reader(&bytes), &window::Cache::default(), &[]).unwrap();
    assert!(
        state
            .remotes
            .verify(&message, expiry + 28_800_000 - 1, true)
    );
    assert!(
        !state
            .remotes
            .verify(&message, expiry + 28_800_000 + 1, true)
    );
}

#[test]
fn configuration_flush_sends_only_the_real_remaining_offset() {
    let mut state = State::default();
    let mut signature = [0; window::SIGNATURE_BYTES];
    signature[0] = 9;
    state.window.observe(signature, false);
    assert!(state.acknowledgement().is_none());
    assert_eq!(state.flush_acknowledgement(), Some(vec![1]));
    assert!(state.flush_acknowledgement().is_none());
    assert_eq!(state.window.update(), [0, 0, 0, 0, 1]);
}

#[test]
fn signed_command_and_player_chat_share_the_native_chain_without_consuming_failed_commands() {
    let now = 1_700_000_000_123;
    let (certificate, roots) = fixture(4_070_908_800_000);
    let public = certificate.private.to_public_key();
    let mut state = State::default();
    state.secure = true;
    state.online = true;
    let announcement = state
        .install(
            crate::auth::chat::Material {
                roots,
                certificate: Some(certificate),
            },
            now,
        )
        .unwrap()
        .unwrap();
    let id: [u8; 16] = announcement[..16].try_into().unwrap();
    let bytes = decode_hex(include_str!("../../../data/command-tree-fixture.hex").trim());
    state.commands = Some(commands::Tree::decode(&mut Reader(&bytes)).unwrap());
    let seen = [6; 256];
    state.window.observe(seen, true);
    assert!(
        state
            .outgoing_command(
                "msg 00000000-0000-0000-0000-000000000007 unsupported",
                now,
                42
            )
            .is_err()
    );
    assert_eq!(state.window.offset(), 1);
    let packet = state.outgoing_command("w Bob hello 夜", now, 42).unwrap();
    assert_eq!(packet.packet, "ServerboundChatCommandSignedPacket");
    let mut r = Reader(&packet.payload);
    assert_eq!(r.string(32767).unwrap(), "w Bob hello 夜");
    assert_eq!(r.i64().unwrap(), now);
    assert_eq!(r.i64().unwrap(), 42);
    assert_eq!(r.var().unwrap(), 1);
    assert_eq!(r.string(16).unwrap(), "message");
    let signature = r.take(256).unwrap();
    let body = security::Body {
        sender: [7; 16],
        session: id,
        index: 0,
        text: "hello 夜",
        timestamp_millis: now,
        salt: 42,
        last_seen: &[seen],
    };
    public
        .verify(
            Pkcs1v15Sign::new::<sha2::Sha256>(),
            &sha2::Sha256::digest(body.payload().unwrap()),
            signature,
        )
        .unwrap();
    assert_eq!(r.0, [1, 0, 0, 8, 32]);
    let next = state.outgoing("after command", now + 1, 43).unwrap();
    let (text, time, salt, signature, _) = read_outgoing(&next);
    let body = security::Body {
        sender: [7; 16],
        session: id,
        index: 1,
        text: &text,
        timestamp_millis: time,
        salt,
        last_seen: &[seen],
    };
    public
        .verify(
            Pkcs1v15Sign::new::<sha2::Sha256>(),
            &sha2::Sha256::digest(body.payload().unwrap()),
            &signature,
        )
        .unwrap();
}
