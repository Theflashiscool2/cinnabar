//! A stopped game consumer must not stop network-thread keepalives or lose ordered facts.
use super::*;

#[tokio::test]
async fn full_game_queue_services_keepalive_and_retains_every_fact_without_duplicate_reply() {
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let (observed, probe) = tokio::sync::oneshot::channel();
        let (finished, finish) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, encrypted).await;
            let mut ack = vec![0, 7];
            for n in [1f64, 64., 2.] {
                ack.extend(n.to_be_bytes());
            }
            ack.extend([0; 8]);
            assert_eq!(wire.read().await.unwrap(), ack);
            // Independent pinned777 SetHealth/KeepAlive IDs and fields. More packets than
            // either the UI channel or the old frame-count-limited deferred queue held.
            for health in 1..=96 {
                let mut packet = vec![0x6a];
                packet.extend((health as f32).to_be_bytes());
                packet.push(20);
                packet.extend(5f32.to_be_bytes());
                wire.write(&packet).await.unwrap();
            }
            let mut keep = vec![0x2d];
            keep.extend(731i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            let mut response = vec![0x1c];
            response.extend(731i64.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), response);
            observed.send(()).unwrap();
            // A later response proves the deferred KeepAlive event was not replied to twice.
            finish.await.unwrap();
            keep[1..].copy_from_slice(&732i64.to_be_bytes());
            response[1..].copy_from_slice(&732i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), response);
        });
        let (events, mut rx) = mpsc::channel(4);
        let (commands, input) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, events, cancel.clone()));
        loop {
            if let Event::Teleport {
                dimension_epoch,
                sequence,
                correction,
            } = rx.recv().await.unwrap().event
            {
                commands
                    .send(session::Command::AcceptTeleport {
                        dimension_epoch,
                        sequence,
                        id: correction.id,
                        position: [1., 64., 2.],
                        yaw: 0.,
                        pitch: 0.,
                    })
                    .await
                    .unwrap();
                break;
            }
        }
        // Deliberately read NO game events until the actual server socket sees the reply.
        tokio::time::timeout(Duration::from_secs(2), probe)
            .await
            .expect("full consumer queue blocked transport keepalive")
            .unwrap();
        for health in 1..=96 {
            let Event::Vitals(vitals) = rx.recv().await.unwrap().event else {
                panic!("lost fact order");
            };
            assert_eq!(vitals.health, health as f32);
        }
        assert!(matches!(
            rx.recv().await.unwrap().event,
            Event::KeepAlive(731)
        ));
        finished.send(()).unwrap();
        server.await.unwrap();
        cancel.cancel();
        let _ = client.await;
    }
}

#[tokio::test]
async fn full_game_queue_preserves_real_disconnect_reason_without_waiting_for_correction() {
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, encrypted).await;
            for id in 1i64..=96 {
                let mut packet = vec![0x2d];
                packet.extend(id.to_be_bytes());
                wire.write(&packet).await.unwrap();
            }
            for id in 1i64..=96 {
                let mut expected = vec![0x1c];
                expected.extend(id.to_be_bytes());
                assert_eq!(wire.read().await.unwrap(), expected);
            }
            // Independent anonymous-NBT string carried by native Disconnect in777.
            let reason = b"fixture disconnect";
            let mut packet = vec![0x20, 8];
            packet.extend((reason.len() as u16).to_be_bytes());
            packet.extend(reason);
            wire.write(&packet).await.unwrap();
            assert!(wire.read().await.is_err());
        });
        let (events, mut rx) = mpsc::channel(4);
        let (_commands, input) = mpsc::channel(4);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, events, Default::default()));
        while !matches!(rx.recv().await.unwrap().event, Event::Teleport { .. }) {}
        // Leave the position unacknowledged and stop consuming the already full game queue.
        let result = tokio::time::timeout(Duration::from_secs(2), client)
            .await
            .expect("terminal server control stalled behind game admission")
            .unwrap();
        assert!(
            matches!(result, Err(session::Error::Disconnect(ref reason)) if reason.contains("fixture disconnect")),
            "{result:?}"
        );
        server.await.unwrap();
    }
}
