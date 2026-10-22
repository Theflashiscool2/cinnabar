use super::*;

/// Encodes a fixture body without its network frame prefix.
fn response(json: &str) -> Vec<u8> {
    let mut body = vec![0];
    codec::write_string(json, MAX_STATUS_UNITS, &mut body).unwrap();
    body
}

#[test]
fn handshake_and_requests_match_independent_wire_vectors() {
    assert_eq!(
        handshake("a", 25565, -1).unwrap(),
        [11, 0, 0xff, 0xff, 0xff, 0xff, 0x0f, 1, b'a', 0x63, 0xdd, 1,]
    );
    assert_eq!(request(), [1, 0]);
    assert_eq!(ping(0x0102030405060708), [9, 1, 1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn invalid_hostnames_fail_before_connecting() {
    for host in [
        String::new(),
        "a\0b".to_owned(),
        "x".repeat(MAX_HOST_UNITS + 1),
    ] {
        assert!(handshake(&host, DEFAULT_PORT, DISCOVERY_PROTOCOL).is_err());
    }
    assert!(
        handshake(
            &"x".repeat(MAX_HOST_UNITS),
            DEFAULT_PORT,
            DISCOVERY_PROTOCOL
        )
        .is_ok()
    );
}

#[test]
fn status_preserves_rich_text_samples_and_extensions() {
    let json = r#"{"version":{"name":"fixture","protocol":123},"players":{"max":20,"online":3,"sample":[{"id":"fixture-id","name":"Player"}]},"description":{"text":"Hello","extra":[{"text":" world","bold":true}]},"enforcesSecureChat":true,"custom":{"x":1}}"#;
    let status = decode_response(&response(json)).unwrap();
    assert_eq!(status.version.as_ref().unwrap().protocol, 123);
    assert_eq!(status.players.as_ref().unwrap().online, 3);
    assert_eq!(
        serde_json::to_value(status).unwrap(),
        serde_json::from_str::<Value>(json).unwrap()
    );
}

#[test]
fn omitted_fields_and_string_descriptions_are_accepted() {
    let status = decode_response(&response(r#"{"description":"Hello"}"#)).unwrap();
    assert!(status.version.is_none());
    assert!(status.players.is_none());
    assert_eq!(status.description, Some(Value::String("Hello".to_owned())));
    assert!(decode_response(&response("{}")).is_ok());
}

#[test]
fn response_rejects_invalid_json_wrong_packet_and_trailing_bytes() {
    for json in [
        "not JSON",
        "null",
        "[]",
        r#"{"version":{"protocol":"bad"}}"#,
    ] {
        assert!(decode_response(&response(json)).is_err());
    }
    let mut body = response("{}");
    body[0] = 1;
    assert!(matches!(
        decode_response(&body),
        Err(ProtocolError::UnexpectedPacket { .. })
    ));
    body[0] = 0;
    body.push(0);
    assert!(matches!(
        decode_response(&body),
        Err(ProtocolError::TrailingData)
    ));
}

#[test]
fn pong_requires_exact_echo_and_length() {
    let packet = ping(-1);
    assert!(check_pong(&packet[1..], -1).is_ok());
    assert!(matches!(
        check_pong(&packet[1..], 0),
        Err(ProtocolError::PongMismatch)
    ));
    assert!(check_pong(&packet[1..8], -1).is_err());
    let mut body = packet[1..].to_vec();
    body.push(0);
    assert!(matches!(
        check_pong(&body, -1),
        Err(ProtocolError::TrailingData)
    ));
}
