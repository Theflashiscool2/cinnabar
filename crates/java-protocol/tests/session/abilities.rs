//! Native ability edges are independent of kinematic correction acknowledgement fences.
use super::*;
use session::Command;

#[tokio::test]
async fn ability_edges_reach_wire_before_pending_correction_ack_and_stale_movement() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let (done, completed) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, true).await;
            assert_eq!(
                wire.read().await.unwrap(),
                containers::outgoing("ServerboundPlayerAbilitiesPacket", &[0])
            );
            assert_eq!(
                wire.read().await.unwrap(),
                containers::outgoing("ServerboundPlayerAbilitiesPacket", &[2])
            );
            done.send(()).unwrap();
            // Cancellation closes the fixture without inventing a teleport acknowledgement.
            assert!(wire.read().await.is_err());
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        while let Some(envelope) = rx.recv().await {
            if let Event::Teleport {
                dimension_epoch, ..
            } = envelope.event
            {
                commands
                    .send(Command::Abilities {
                        dimension_epoch: 99,
                        flying: false,
                    })
                    .await
                    .unwrap();
                commands
                    .send(Command::Move {
                        dimension_epoch,
                        sequence: 0,
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
                for flying in [false, true] {
                    commands
                        .send(Command::Abilities {
                            dimension_epoch,
                            flying,
                        })
                        .await
                        .unwrap();
                }
                break;
            }
        }
        completed.await.unwrap();
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
