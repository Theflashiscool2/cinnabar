//! Native logical tick boundaries are independent of player placement and loaded terrain.
use super::*;

#[tokio::test]
async fn loading_and_retired_poses_close_once_and_replaced_world_ticks_are_discarded() {
    for encrypted in [false, true] {
        tokio::time::timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string().parse().unwrap();
            let cancel = CancellationToken::new();
            let child = cancel.clone();
            let server = tokio::spawn(async move {
                let mut wire = through_initial_position(listener, encrypted).await;
                for epoch in 1..=2 {
                    // Independent pinned protocol777 ID13: empty ClientTickEnd, before load/ACK.
                    assert_eq!(wire.read().await.unwrap(), [0x0d]);
                    let mut ack = vec![0, 7];
                    for x in [1f64, 64., 2.] {
                        ack.extend(x.to_be_bytes());
                    }
                    ack.extend([0; 8]);
                    assert_eq!(wire.read().await.unwrap(), ack);
                    // The stale pose must NOT serialize input, sprint, status or coordinates.
                    assert_eq!(wire.read().await.unwrap(), [0x0d]);
                    assert_eq!(wire.read().await.unwrap(), [0x2c]);
                    assert_eq!(wire.read().await.unwrap(), [0x21, 1]);
                    assert_eq!(wire.read().await.unwrap(), [0x0d]);
                    assert_eq!(wire.read().await.unwrap(), [0x0d]);
                    if epoch == 1 {
                        wire.write(&world_login()).await.unwrap();
                        let mut position = vec![0x49, 7];
                        for n in [1f64, 64., 2., 0., 0., 0.] {
                            position.extend(n.to_be_bytes());
                        }
                        position.extend([0; 12]);
                        wire.write(&position).await.unwrap();
                    }
                }
                // A final native keepalive proves there are no duplicate/trailing tick ends.
                let mut keep = vec![0x2d];
                keep.extend(321i64.to_be_bytes());
                wire.write(&keep).await.unwrap();
                let mut expected = vec![0x1c];
                expected.extend(321i64.to_be_bytes());
                assert_eq!(wire.read().await.unwrap(), expected);
                child.cancel();
            });
            let (tx, mut rx) = mpsc::channel(32);
            let (commands, input) = mpsc::channel(16);
            let mut options = Options::fixture(address, "Fixture".into(), 7);
            options.commands = Some(input);
            let client = tokio::spawn(session::run(options, tx, cancel.clone()));
            while let Some(envelope) = rx.recv().await {
                if let Event::Teleport {
                    dimension_epoch,
                    sequence,
                    correction,
                } = envelope.event
                {
                    for epoch in [dimension_epoch.saturating_sub(1), dimension_epoch] {
                        commands
                            .send(session::Command::ClientTickEnd {
                                dimension_epoch: epoch,
                            })
                            .await
                            .unwrap();
                    }
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
                    commands
                        .send(session::Command::Move {
                            dimension_epoch,
                            sequence: 0,
                            frame: session::movement::Frame {
                                entity: 42,
                                position: Some([999.; 3]),
                                rotation: None,
                                on_ground: true,
                                status_changed: true,
                                horizontal_collision: false,
                                input: Some(65),
                                sprinting: Some(true),
                            },
                        })
                        .await
                        .unwrap();
                    commands
                        .send(session::Command::PlayerLoaded { dimension_epoch })
                        .await
                        .unwrap();
                    commands
                        .send(session::Command::Move {
                            dimension_epoch,
                            sequence,
                            frame: session::movement::Frame {
                                entity: 42,
                                position: None,
                                rotation: None,
                                on_ground: true,
                                status_changed: true,
                                horizontal_collision: false,
                                input: None,
                                sprinting: None,
                            },
                        })
                        .await
                        .unwrap();
                    commands
                        .send(session::Command::ClientTickEnd { dimension_epoch })
                        .await
                        .unwrap();
                }
            }
            server.await.unwrap();
            let _ = client.await;
        })
        .await
        .expect("tick boundary transport probe timed out");
    }
}
