//! Scoped entity-use writes through real encrypted framed sockets.
use super::*;
fn spawn() -> Vec<u8> {
    let mut packet = vec![1, 99];
    packet.extend([0x33; 16]);
    packet.push(101); // official26.3 pig
    for n in [1f64, 64., 2.] {
        packet.extend(n.to_be_bytes());
    }
    packet.extend([0; 5]);
    packet
}
#[tokio::test]
async fn entity_use_rejects_old_spawn_tokens_epochs_and_corrections() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, true).await;
            let mut ack = vec![0, 7];
            for n in [1f64, 64., 2.] {
                ack.extend(n.to_be_bytes());
            }
            ack.extend([0; 8]);
            assert_eq!(wire.read().await.unwrap(), ack);
            assert_eq!(wire.read().await.unwrap(), [0x2c]);
            let expected = [0x1a, 99, 0, 0, 0]; // native Interact, id99, main, zero-LP-point, secondaryfalse
            wire.write(&spawn()).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), expected);
            wire.write(&[0x4e, 1, 99]).await.unwrap();
            wire.write(&spawn()).await.unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                expected,
                "old target must not leak into reused id"
            );
            let mut keep = vec![0x2d];
            keep.extend(987i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            let mut reply = vec![0x1c];
            reply.extend(987i64.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), reply);
            child.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(16);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut previous = None;
        let mut spawns = 0;
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Teleport {
                    dimension_epoch,
                    sequence,
                    correction,
                } => {
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
                        .send(session::Command::PlayerLoaded { dimension_epoch })
                        .await
                        .unwrap();
                }
                Event::Entities {
                    dimension_epoch,
                    packet: session::entities::Packet::Spawn(s),
                } => {
                    spawns += 1;
                    for (epoch, correction, lifetime) in [
                        (99, 1, s.lifetime),
                        (dimension_epoch, 99, s.lifetime),
                        (dimension_epoch, 1, previous.unwrap_or(0)),
                        (dimension_epoch, 1, s.lifetime),
                    ] {
                        commands
                            .send(session::Command::UseEntity {
                                dimension_epoch: epoch,
                                correction_sequence: correction,
                                lifetime,
                                action: session::interaction::EntityUse {
                                    entity: s.id,
                                    hand: session::interaction::Hand::Main,
                                    location: [0.; 3],
                                    secondary: false,
                                },
                            })
                            .await
                            .unwrap();
                    }
                    previous = Some(s.lifetime);
                }
                Event::KeepAlive(987) => break,
                Event::Phase(_)
                | Event::Bootstrap(_)
                | Event::Entities {
                    packet: session::entities::Packet::Remove(_),
                    ..
                } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(spawns, 2);
        cancel.cancel();
        assert!(matches!(
            client.await.unwrap(),
            Err(session::Error::Cancelled)
        ));
        server.await.unwrap();
    })
    .await
    .unwrap();
}
