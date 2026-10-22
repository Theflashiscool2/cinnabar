use super::*;
use tokio::io::duplex;

/// A minimal independently encoded status response, followed by a pong for token zero.
const RESPONSE: &[u8] = b"\x04\x00\x02{}";
const PONG: &[u8] = &[9, 1, 0, 0, 0, 0, 0, 0, 0, 0];

#[tokio::test]
async fn tcp_probe_sends_status_not_login_and_preserves_virtual_host() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = async {
        let (mut peer, _) = listener.accept().await.unwrap();
        let body = read_frame(&mut peer).await.unwrap();
        let mut bytes = body.as_slice();
        assert_eq!(codec::read_varint(&mut bytes).unwrap(), 0);
        assert_eq!(codec::read_varint(&mut bytes).unwrap(), -1);
        assert_eq!(
            codec::read_string(&mut bytes, status::MAX_HOST_UNITS).unwrap(),
            "127.0.0.1"
        );
        assert_eq!(bytes, &[port.to_be_bytes()[0], port.to_be_bytes()[1], 1]);
        let mut request = [0; 2];
        peer.read_exact(&mut request).await.unwrap();
        assert_eq!(request, [1, 0]);
        // Individual writes plus a small in-memory test below exercise fragmented reads.
        peer.write_all(RESPONSE).await.unwrap();
        let mut ping = [0; 10];
        peer.read_exact(&mut ping).await.unwrap();
        assert_eq!(ping, PONG);
        peer.write_all(PONG).await.unwrap();
    };
    let client = query_status("127.0.0.1", port, -1, Duration::from_secs(5));
    let ((), report) = timeout(Duration::from_secs(10), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    assert!(report.unwrap().status.version.is_none());
}

#[tokio::test]
async fn fragmented_frames_and_coalesced_frames_keep_boundaries() {
    // Capacity one guarantees partial reads/writes without sleeps or wall-time assumptions.
    let (mut reader, mut writer) = duplex(1);
    let sending = async {
        writer.write_all(RESPONSE).await.unwrap();
        writer.write_all(PONG).await.unwrap();
    };
    let reading = async {
        assert_eq!(read_frame(&mut reader).await.unwrap(), &RESPONSE[1..]);
        assert_eq!(read_frame(&mut reader).await.unwrap(), &PONG[1..]);
    };
    tokio::join!(sending, reading);
    let combined = [RESPONSE, PONG].concat();
    let mut reader = combined.as_slice();
    assert_eq!(read_frame(&mut reader).await.unwrap(), &RESPONSE[1..]);
    assert_eq!(read_frame(&mut reader).await.unwrap(), &PONG[1..]);
    assert!(reader.is_empty());
}

#[tokio::test]
async fn invalid_or_truncated_frames_fail_without_waiting_for_payload() {
    for bytes in [&[0][..], &[0x80, 0x80, 0x80], &[5, 0], &[0x80]] {
        let mut input = bytes;
        assert!(read_frame(&mut input).await.is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn stalled_exchange_obeys_a_total_deadline() {
    let (mut client, _peer) = duplex(1024);
    let handshake = status::handshake("localhost", status::DEFAULT_PORT, -1).unwrap();
    assert!(
        timeout(Duration::from_secs(1), exchange(&mut client, &handshake))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn wrong_pong_is_a_protocol_failure() {
    let (mut client, mut peer) = duplex(1024);
    let handshake = status::handshake("localhost", status::DEFAULT_PORT, -1).unwrap();
    let server = async {
        let mut wrong = PONG.to_vec();
        *wrong.last_mut().unwrap() = 1;
        peer.write_all(RESPONSE).await.unwrap();
        peer.write_all(&wrong).await.unwrap();
        // Keep the read side alive until the client's writes are complete.
        read_frame(&mut peer).await.unwrap();
        read_frame(&mut peer).await.unwrap();
        read_frame(&mut peer).await.unwrap();
    };
    let (result, ()) = tokio::join!(exchange(&mut client, &handshake), server);
    assert!(matches!(
        result,
        Err(QueryError::Protocol(ProtocolError::PongMismatch))
    ));
}

#[tokio::test]
async fn launcher_keeps_status_when_server_closes_before_pong() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port())
        .parse()
        .unwrap();
    let server = async {
        let (mut peer, _) = listener.accept().await.unwrap();
        read_frame(&mut peer).await.unwrap();
        read_frame(&mut peer).await.unwrap();
        peer.write_all(RESPONSE).await.unwrap();
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let ((), result) = tokio::join!(
        server,
        super::discover(&address, Duration::from_secs(5), &cancel)
    );
    assert!(result.unwrap().latency.is_none());
}
