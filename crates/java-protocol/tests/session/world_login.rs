//! Repeated play-login wire probe, including encrypted transport and retired-world receipts.
use super::*;

async fn replacement(encrypted: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, encrypted).await;
            for epoch in 1..=3 {
                let mut expected = vec![0, 7];
                for coordinate in [1f64, 64., 2.] {
                    expected.extend(coordinate.to_be_bytes());
                }
                expected.extend([0; 8]);
                assert_eq!(wire.read().await.unwrap(), expected);
                assert_eq!(wire.read().await.unwrap(), [0x2c]);
                if epoch < 3 {
                    // Independent protocol-777 death packet; a new login must clear dead state.
                    wire.write(&[0x45, (42 * epoch) as u8, 8, 0, 0])
                        .await
                        .unwrap();
                    let mut login = world_login();
                    login[1..5].copy_from_slice(&(42i32 * (epoch + 1)).to_be_bytes());
                    wire.write(&login).await.unwrap();
                    let mut position = vec![0x49, 7]; // ID is deliberately reused.
                    for n in [1f64, 64., 2., 0., 0., 0.] {
                        position.extend(n.to_be_bytes());
                    }
                    position.extend([0; 12]);
                    wire.write(&position).await.unwrap();
                }
            }
            let mut keep = vec![0x2d];
            keep.extend(123i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            let mut expected = vec![0x1c];
            expected.extend(123i64.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), expected);
            child.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(16);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut bootstraps = 0;
        let mut replacements = 0;
        let mut corrections = 0;
        while let Some(envelope) = rx.recv().await {
            assert_eq!(envelope.generation, 7);
            match envelope.event {
                Event::Bootstrap(b) => {
                    bootstraps += 1;
                    assert_eq!(b.world.entity_id, 42);
                }
                Event::WorldLogin(world) => {
                    replacements += 1;
                    assert_eq!(world.entity_id, 42 * (replacements + 1));
                }
                Event::Teleport {
                    dimension_epoch,
                    sequence,
                    correction,
                } => {
                    corrections += 1;
                    assert_eq!((dimension_epoch, sequence), (corrections, corrections));
                    if dimension_epoch > 1 {
                        commands
                            .send(session::Command::PlayerLoaded {
                                dimension_epoch: dimension_epoch - 1,
                            })
                            .await
                            .unwrap();
                        commands
                            .send(session::Command::AcceptTeleport {
                                dimension_epoch: dimension_epoch - 1,
                                sequence: sequence - 1,
                                id: 7,
                                position: [99.; 3],
                                yaw: 0.,
                                pitch: 0.,
                            })
                            .await
                            .unwrap();
                        commands.send(session::Command::Respawn).await.unwrap();
                    }
                    commands
                        .send(session::Command::AcceptTeleport {
                            dimension_epoch,
                            sequence,
                            id: correction.id,
                            position: correction.position,
                            yaw: correction.yaw,
                            pitch: correction.pitch,
                        })
                        .await
                        .unwrap();
                    commands
                        .send(session::Command::PlayerLoaded { dimension_epoch })
                        .await
                        .unwrap();
                }
                Event::KeepAlive(123) => break,
                Event::Phase(_) | Event::Death { .. } => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!((bootstraps, replacements, corrections), (1, 2, 3));
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
#[tokio::test]
async fn repeated_play_login_retires_world_receipts_without_reauthenticating() {
    replacement(false).await;
}
#[tokio::test]
async fn repeated_play_login_preserves_encryption_and_compression() {
    replacement(true).await;
}
