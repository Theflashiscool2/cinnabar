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
            // New capability packets remain behind the outstanding correction. Ability
            // commands retain the last admitted revision until the owner acknowledges it.
            let mut replacement = Vec::new();
            codec::write_varint(
                java_protocol::target::game::clientbound::NAMES
                    .iter()
                    .position(|n| *n == "ClientboundPlayerAbilitiesPacket")
                    .unwrap() as i32,
                &mut replacement,
            );
            replacement.push(7);
            replacement.extend(0.05_f32.to_be_bytes());
            replacement.extend(0.1_f32.to_be_bytes());
            wire.write(&replacement).await.unwrap();
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
                        authority_revision: 0,
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
                            authority_revision: 0,
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

#[tokio::test]
async fn replaced_grant_rejects_old_disable_on_same_epoch_with_a_not_written_receipt() {
    replaced_grant_case(false).await;
}

#[tokio::test]
async fn ready_unread_mode_and_grant_precede_an_old_ability_command() {
    replaced_grant_case(true).await;
}

async fn replaced_grant_case(queue_before_mode_admission: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let (ready, granted) = tokio::sync::oneshot::channel();
        let mut granted = Some(granted);
        let (done, completed) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, true).await;
            // The test domain resolves the real initial correction before these play updates.
            let mut ack = vec![0, 7];
            for coordinate in [1_f64, 64., 2.] {
                ack.extend(coordinate.to_be_bytes());
            }
            ack.extend([0; 8]);
            assert_eq!(wire.read().await.unwrap(), ack);
            let mut payload = vec![7];
            payload.extend(0.05_f32.to_be_bytes());
            payload.extend(0.1_f32.to_be_bytes());
            let ability_id = java_protocol::target::game::clientbound::NAMES
                .iter()
                .position(|n| *n == "ClientboundPlayerAbilitiesPacket")
                .unwrap();
            let mode_id = java_protocol::target::game::clientbound::NAMES
                .iter()
                .position(|n| *n == "ClientboundGameEventPacket")
                .unwrap();
            let incoming = |id, payload: &[u8]| {
                let mut bytes = Vec::new();
                codec::write_varint(id as i32, &mut bytes);
                bytes.extend(payload);
                bytes
            };
            wire.write(&incoming(ability_id, &payload)).await.unwrap();
            let mut mode = vec![3];
            mode.extend(3_f32.to_be_bytes());
            wire.write(&incoming(mode_id, &mode)).await.unwrap();
            wire.write(&incoming(ability_id, &payload)).await.unwrap();
            // A repeated identical grant must also invalidate a previously derived edge.
            wire.write(&incoming(ability_id, &payload)).await.unwrap();
            ready.send(()).unwrap();
            // Only legitimate current-authority edges reach the real encrypted socket.
            for flying in [true, false] {
                assert_eq!(
                    wire.read().await.unwrap(),
                    containers::outgoing(
                        "ServerboundPlayerAbilitiesPacket",
                        &[u8::from(flying) << 1]
                    )
                );
            }
            done.send(()).unwrap();
            assert!(wire.read().await.is_err());
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut epoch = 0;
        let mut held_old = None;
        let mut receipts = Vec::new();
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Teleport {
                    dimension_epoch,
                    sequence,
                    correction,
                } => {
                    epoch = dimension_epoch;
                    commands
                        .send(Command::AcceptTeleport {
                            dimension_epoch,
                            sequence,
                            id: correction.id,
                            position: correction.position,
                            yaw: correction.yaw,
                            pitch: correction.pitch,
                        })
                        .await
                        .unwrap();
                }
                Event::Abilities {
                    authority_revision: 1,
                    ..
                } => {
                    // Retain the action derived from the first flags7 receipt while the
                    // separate domain owner has not yet admitted the following mode.
                    held_old = Some(Command::Abilities {
                        dimension_epoch: epoch,
                        authority_revision: 1,
                        flying: false,
                    });
                    if queue_before_mode_admission {
                        // The complete replacement frames have been written while the domain
                        // still holds only values1. The decoder may not have selected them yet.
                        granted.take().unwrap().await.unwrap();
                        commands.send(held_old.take().unwrap()).await.unwrap();
                    }
                }
                Event::Abilities {
                    authority_revision: 2,
                    ..
                } => {
                    // Queue it before the domain consumes replacement values3/4: the socket
                    // must reject it against decoded mode authority even under this ordering.
                    if !queue_before_mode_admission {
                        commands.send(held_old.take().unwrap()).await.unwrap();
                    }
                }
                Event::AbilitiesWritten(receipt) => {
                    receipts.push((receipt.authority_revision, receipt.flying, receipt.written));
                }
                Event::Abilities {
                    authority_revision: 4,
                    ..
                } => break,
                _ => {}
            }
        }
        if let Some(granted) = granted {
            granted.await.unwrap();
        }
        for (authority_revision, flying) in [(3, false), (4, true), (4, false)] {
            commands
                .send(Command::Abilities {
                    dimension_epoch: epoch,
                    authority_revision,
                    flying,
                })
                .await
                .unwrap();
        }
        while receipts.len() < 4 {
            if let Event::AbilitiesWritten(receipt) = rx.recv().await.unwrap().event {
                receipts.push((receipt.authority_revision, receipt.flying, receipt.written));
            }
        }
        assert_eq!(
            receipts,
            [
                (1, false, false),
                (3, false, false),
                (4, true, true),
                (4, false, true)
            ]
        );
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

#[tokio::test]
async fn game_thread_ping_defers_replacement_authority_without_overtaking_ability_fifo() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let (done, completed) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let mut wire = through_initial_position(listener, true).await;
            let mut ack = vec![0, 7];
            for value in [1_f64, 64., 2.] {
                ack.extend(value.to_be_bytes());
            }
            ack.extend([0; 8]);
            assert_eq!(wire.read().await.unwrap(), ack);
            let incoming = |name: &str, payload: &[u8]| {
                let id = java_protocol::target::game::clientbound::NAMES
                    .iter()
                    .position(|n| *n == name)
                    .unwrap();
                let mut bytes = Vec::new();
                codec::write_varint(id as i32, &mut bytes);
                bytes.extend(payload);
                bytes
            };
            wire.write(&incoming("ClientboundPingPacket", &123_i32.to_be_bytes()))
                .await
                .unwrap();
            let mut values = vec![7];
            values.extend(0.05_f32.to_be_bytes());
            values.extend(0.1_f32.to_be_bytes());
            wire.write(&incoming("ClientboundPlayerAbilitiesPacket", &values))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                containers::outgoing("ServerboundPlayerAbilitiesPacket", &[0])
            );
            assert_eq!(
                wire.read().await.unwrap(),
                containers::outgoing("ServerboundPongPacket", &123_i32.to_be_bytes())
            );
            done.send(()).unwrap();
            assert!(wire.read().await.is_err());
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut written = false;
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Teleport {
                    dimension_epoch,
                    sequence,
                    correction,
                } => {
                    commands
                        .send(Command::AcceptTeleport {
                            dimension_epoch,
                            sequence,
                            id: correction.id,
                            position: correction.position,
                            yaw: correction.yaw,
                            pitch: correction.pitch,
                        })
                        .await
                        .unwrap();
                }
                Event::Ping { sequence, id } => {
                    commands
                        .send(Command::Abilities {
                            dimension_epoch: 1,
                            authority_revision: 0,
                            flying: false,
                        })
                        .await
                        .unwrap();
                    commands.send(Command::Pong { sequence, id }).await.unwrap();
                }
                Event::AbilitiesWritten(receipt) => {
                    assert_eq!(receipt.authority_revision, 0);
                    assert!(receipt.written);
                    written = true;
                }
                Event::Abilities {
                    authority_revision: 1,
                    ..
                } => {
                    assert!(
                        written,
                        "replacement authority may advance only after the head ability and Pong"
                    );
                    break;
                }
                _ => {}
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
