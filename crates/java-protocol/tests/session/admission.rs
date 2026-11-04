//! Control/game dispatch through real framed sockets; no simulated successful gameplay.
use super::*;

#[tokio::test]
async fn keep_alive_survives_delayed_admission_without_overtaking_ping_or_relative_corrections() {
    for encrypted in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let (keep_tx, keep_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, encrypted).await;
            // Independent protocol777 IDs from the pinned MCProtocolLib declaration.
            let mut keep = vec![0x2d];
            keep.extend(99i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            let mut ping = vec![0x3e];
            ping.extend(123i32.to_be_bytes());
            wire.write(&ping).await.unwrap();
            let mut relative = vec![0x49, 7];
            for n in [2f64, 0., 0., 0., 0., 0.] {
                relative.extend(n.to_be_bytes());
            }
            relative.extend([0; 8]);
            relative.extend(1i32.to_be_bytes()); // relativeX only
            wire.write(&relative).await.unwrap();
            let mut response = vec![0x1c];
            response.extend(99i64.to_be_bytes());
            assert_eq!(
                wire.read().await.unwrap(),
                response,
                "keep-alive waits for neither renderer nor player acknowledgement"
            );
            keep_tx.send(()).unwrap();
            let mut first = vec![0, 7];
            for n in [1f64, 64., 2.] {
                first.extend(n.to_be_bytes());
            }
            first.extend([0; 8]);
            assert_eq!(
                wire.read().await.unwrap(),
                first,
                "no pong or stale/guessed position before admitted correction"
            );
            let mut pong = vec![0x2d];
            pong.extend(123i32.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), pong);
            let mut second = vec![0, 7];
            for n in [3f64, 0., 0.] {
                second.extend(n.to_be_bytes());
            }
            second.extend([0; 8]);
            assert_eq!(
                wire.read().await.unwrap(),
                second,
                "reused teleport ID retains a distinct sequence and resolved relative anchor"
            );
            done_tx.send(()).unwrap();
            assert!(wire.read().await.is_err());
        });
        let (events, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, events, cancel.clone()));
        let (epoch, sequence) = loop {
            if let Event::Teleport {
                dimension_epoch,
                sequence,
                ..
            } = rx.recv().await.unwrap().event
            {
                break (dimension_epoch, sequence);
            }
        };
        tokio::time::timeout(Duration::from_secs(2), keep_rx)
            .await
            .expect("network control blocked behind unadmitted correction")
            .unwrap();
        // A delayed owner remains live beyond the old ten-second renderer-dependent deadline.
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(11)).await;
        tokio::time::resume();
        commands
            .send(session::Command::AcceptTeleport {
                dimension_epoch: epoch,
                sequence,
                id: 8,
                position: [1., 64., 2.],
                yaw: 0.,
                pitch: 0.,
            })
            .await
            .unwrap();
        commands
            .send(session::Command::AcceptTeleport {
                dimension_epoch: epoch,
                sequence,
                id: 7,
                position: [1., 64., 2.],
                yaw: 0.,
                pitch: 0.,
            })
            .await
            .unwrap();
        let ping_sequence = loop {
            if let Event::Ping { sequence, id } = rx.recv().await.unwrap().event {
                assert_eq!(id, 123);
                break sequence;
            }
        };
        commands
            .send(session::Command::Pong {
                sequence: ping_sequence,
                id: 123,
            })
            .await
            .unwrap();
        let next = loop {
            if let Event::Teleport {
                dimension_epoch,
                sequence,
                correction,
            } = rx.recv().await.unwrap().event
            {
                assert_eq!(correction.position, [2., 0., 0.]);
                break (dimension_epoch, sequence);
            }
        };
        assert!(next.1 > sequence);
        commands
            .send(session::Command::AcceptTeleport {
                dimension_epoch: next.0,
                sequence: next.1,
                id: 7,
                position: [3., 0., 0.],
                yaw: 0.,
                pitch: 0.,
            })
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), done_rx)
            .await
            .unwrap()
            .unwrap();
        cancel.cancel();
        assert!(matches!(
            client.await.unwrap(),
            Err(session::Error::Cancelled)
        ));
        server.await.unwrap();
    }
}
