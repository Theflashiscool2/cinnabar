//! Neutral cancellation must survive a pending correction and first-world loading.
use super::containers::{incoming, outgoing};
use super::*;

#[tokio::test]
async fn item_release_waits_for_loading_but_not_a_position_correction_token() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, true).await;
            let mut ack = vec![0, 7];
            for coordinate in [1f64, 64., 2.] {
                ack.extend(coordinate.to_be_bytes());
            }
            ack.extend([0; 8]);
            assert_eq!(wire.read().await.unwrap(), ack);
            assert_eq!(wire.read().await.unwrap(), [0x2c]);
            let fixture: serde_json::Value =
                serde_json::from_str(include_str!("../fixtures/item-use-lifecycle-777.json"))
                    .unwrap();
            let release: Vec<u8> = serde_json::from_value(fixture["release_use"].clone()).unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundPlayerActionPacket", &release)
            );
            let keep = 891i64.to_be_bytes();
            wire.write(&incoming("ClientboundKeepAlivePacket", &keep))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundKeepAlivePacket", &keep),
                "no extra punch or duplicate release"
            );
            child.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(16);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut released = false;
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Teleport {
                    dimension_epoch,
                    sequence,
                    correction,
                } => {
                    commands
                        .send(session::Command::Interact {
                            dimension_epoch,
                            correction_sequence: u64::MAX,
                            action: session::interaction::Dig::release_use(),
                        })
                        .await
                        .unwrap();
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
                    released = true;
                }
                Event::KeepAlive(891) => break,
                Event::Phase(_) | Event::Bootstrap(_) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(released);
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
