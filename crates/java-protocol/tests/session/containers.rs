//! Synthetic encrypted native-wire tests; not vanilla gameplay or GUI acceptance.
use super::*;
use session::Command;
use session::inventory::{
    Packet,
    outbound::{self, HashedStack, Pickup},
};
use std::collections::BTreeMap;
fn incoming(name: &str, body: &[u8]) -> Vec<u8> {
    let id = java_protocol::target::game::clientbound::NAMES
        .iter()
        .position(|v| *v == name)
        .unwrap();
    let mut packet = Vec::new();
    codec::write_varint(id as i32, &mut packet);
    packet.extend(body);
    packet
}
fn outgoing(name: &str, body: &[u8]) -> Vec<u8> {
    let id = java_protocol::target::game::serverbound::NAMES
        .iter()
        .position(|v| *v == name)
        .unwrap();
    let mut packet = Vec::new();
    codec::write_varint(id as i32, &mut packet);
    packet.extend(body);
    packet
}
fn open() -> Vec<u8> {
    let kind = java_protocol::target::CONTAINER_TYPES
        .iter()
        .position(|v| *v == "GENERIC_9X3")
        .unwrap();
    let mut body = vec![7];
    codec::write_varint(kind as i32, &mut body);
    body.extend([8, 0, 5]);
    body.extend(b"Chest");
    incoming("ClientboundOpenScreenPacket", &body)
}
fn contents() -> Vec<u8> {
    let mut body = vec![7, 129, 1, 63, 8, 1, 0, 0]; // state129; slot0 stone8, empty patch
    body.extend(std::iter::repeat_n(0, 62));
    body.push(0);
    incoming("ClientboundContainerSetContentPacket", &body)
}
#[tokio::test]
async fn reused_menu_id_retires_queued_clicks_and_close_receipts_are_not_server_acks() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundPlayerLoadedPacket", &[])
            );
            wire.write(&open()).await.unwrap();
            wire.write(&contents()).await.unwrap();
            // Independent pinned-native codec output (InspectContainerPackets), not encoder output.
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing(
                    "ServerboundContainerClickPacket",
                    &[7, 129, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 8, 0, 0]
                )
            );
            wire.write(&open()).await.unwrap();
            // The stale old-menu click must produce NO wire frame, even though the ID is reused.
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundContainerClosePacket", &[7])
            );
            let mut keep = Vec::new();
            keep.extend(123i64.to_be_bytes());
            wire.write(&incoming("ClientboundKeepAlivePacket", &keep))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundKeepAlivePacket", &keep)
            );
            stop.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let (commands, input) = mpsc::channel(8);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let click = outbound::pickup(&Pickup {
            container: 7,
            state: 129,
            slot: 0,
            button: 0,
            changed: BTreeMap::from([(0, None)]),
            carried: Some(HashedStack {
                item: 1,
                count: 8,
                added: BTreeMap::new(),
                removed: Vec::new(),
            }),
        })
        .unwrap();
        let mut openings = Vec::new();
        let mut receipts = Vec::new();
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Bootstrap(_) => commands
                    .send(Command::PlayerLoaded { dimension_epoch: 1 })
                    .await
                    .unwrap(),
                Event::Inventory {
                    incarnation,
                    packet: Packet::Open { .. },
                    ..
                } => {
                    openings.push(incarnation);
                    if openings.len() == 2 {
                        commands
                            .send(Command::ContainerClick {
                                dimension_epoch: 1,
                                incarnation: openings[0],
                                sequence: 2,
                                click: outbound::quick_craft(
                                    7,
                                    129,
                                    &[1, 2],
                                    false,
                                    HashedStack {
                                        item: 1,
                                        count: 11,
                                        added: BTreeMap::new(),
                                        removed: Vec::new(),
                                    },
                                )
                                .unwrap(),
                            })
                            .await
                            .unwrap();
                        commands
                            .send(Command::ContainerClose {
                                dimension_epoch: 1,
                                incarnation,
                                sequence: 3,
                                container: 7,
                            })
                            .await
                            .unwrap();
                    }
                }
                Event::Inventory {
                    incarnation,
                    packet: Packet::Content { container: 7, .. },
                    ..
                } => {
                    commands
                        .send(Command::ContainerClick {
                            dimension_epoch: 1,
                            incarnation,
                            sequence: 1,
                            click: click.clone(),
                        })
                        .await
                        .unwrap();
                }
                Event::InventorySent {
                    sequence,
                    written,
                    closed,
                    ..
                } => receipts.push((sequence, written, closed)),
                Event::KeepAlive(123) => break,
                _ => {}
            }
        }
        assert_eq!(openings, [1, 2]);
        assert_eq!(
            receipts,
            [(1, true, false), (2, false, false), (3, true, true)]
        );
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
async fn quick_craft_phases_are_contiguous_before_the_next_queued_close() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundPlayerLoadedPacket", &[])
            );
            wire.write(&open()).await.unwrap();
            wire.write(&contents()).await.unwrap();
            for (slot, button) in [
                ([0xfc, 0x19], 0),
                ([0, 1], 1),
                ([0, 2], 1),
                ([0xfc, 0x19], 2),
            ] {
                assert_eq!(
                    wire.read().await.unwrap(),
                    outgoing(
                        "ServerboundContainerClickPacket",
                        &[7, 129, 1, slot[0], slot[1], button, 5, 0, 1, 1, 11, 0, 0]
                    )
                );
            }
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundContainerClosePacket", &[7])
            );
            wire.write(&incoming(
                "ClientboundKeepAlivePacket",
                &456i64.to_be_bytes(),
            ))
            .await
            .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundKeepAlivePacket", &456i64.to_be_bytes())
            );
            stop.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let (commands, input) = mpsc::channel(1);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut receipts = Vec::new();
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Bootstrap(_) => commands
                    .send(Command::PlayerLoaded { dimension_epoch: 1 })
                    .await
                    .unwrap(),
                Event::Inventory {
                    incarnation,
                    packet: Packet::Content { .. },
                    ..
                } => {
                    commands
                        .send(Command::ContainerClick {
                            dimension_epoch: 1,
                            incarnation,
                            sequence: 1,
                            click: outbound::quick_craft(
                                7,
                                129,
                                &[1, 2],
                                false,
                                HashedStack {
                                    item: 1,
                                    count: 11,
                                    added: BTreeMap::new(),
                                    removed: Vec::new(),
                                },
                            )
                            .unwrap(),
                        })
                        .await
                        .unwrap();
                    commands
                        .send(Command::ContainerClose {
                            dimension_epoch: 1,
                            incarnation,
                            sequence: 2,
                            container: 7,
                        })
                        .await
                        .unwrap();
                }
                Event::InventorySent {
                    sequence, written, ..
                } => receipts.push((sequence, written)),
                Event::KeepAlive(456) => break,
                _ => {}
            }
        }
        assert_eq!(receipts, [(1, true), (2, true)]);
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
async fn entity_attack_tokens_fence_reuse_and_preserve_attack_punch_selection_order() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundPlayerLoadedPacket", &[])
            );
            // Independent pinned AddEntity codec fields: ID300, repeated UUID, pig101,
            // three f64 zero coordinates, packed zero velocity, three angles, object-data0.
            let mut spawn = vec![172, 2];
            spawn.extend([9; 16]);
            spawn.push(101);
            spawn.extend([0; 24]);
            spawn.extend([0; 5]);
            wire.write(&incoming("ClientboundAddEntityPacket", &spawn))
                .await
                .unwrap();
            wire.write(&incoming("ClientboundRemoveEntitiesPacket", &[1, 172, 2]))
                .await
                .unwrap();
            wire.write(&incoming("ClientboundAddEntityPacket", &spawn))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundAttackPacket", &[172, 2])
            );
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundPunchPacket", &[])
            );
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundSetCarriedItemPacket", &[0, 0])
            );
            stop.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let (commands, input) = mpsc::channel(8);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut first = None;
        let mut verified = false;
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Bootstrap(_) => commands
                    .send(Command::PlayerLoaded { dimension_epoch: 1 })
                    .await
                    .unwrap(),
                Event::Entities {
                    packet: session::entities::Packet::Spawn(s),
                    ..
                } => {
                    if let Some(old) = first {
                        assert_ne!(old, s.lifetime);
                        for (epoch, correction, lifetime) in [
                            (1, 0, old),
                            (2, 0, s.lifetime),
                            (1, 1, s.lifetime),
                            (1, 0, s.lifetime),
                        ] {
                            commands
                                .send(Command::Attack {
                                    dimension_epoch: epoch,
                                    correction_sequence: correction,
                                    entity: 300,
                                    lifetime,
                                })
                                .await
                                .unwrap();
                        }
                        commands
                            .send(Command::Select {
                                dimension_epoch: 1,
                                sequence: 1,
                                slot: 0,
                            })
                            .await
                            .unwrap();
                    } else {
                        first = Some(s.lifetime);
                    }
                }
                Event::SelectionSent { sequence: 1, .. } => {
                    verified = true;
                    break;
                }
                _ => {}
            }
        }
        cancel.cancel();
        server.await.unwrap();
        let _ = client.await.unwrap();
        assert!(verified);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stationary_native_tick_sends_tick_end_without_an_extra_status_only_packet() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundPlayerLoadedPacket", &[])
            );
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundClientTickEndPacket", &[])
            );
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundSetCarriedItemPacket", &[0, 0])
            );
            stop.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let (commands, input) = mpsc::channel(8);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let mut verified = false;
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                Event::Bootstrap(_) => {
                    commands
                        .send(Command::PlayerLoaded { dimension_epoch: 1 })
                        .await
                        .unwrap();
                    commands
                        .send(Command::Move {
                            dimension_epoch: 1,
                            sequence: 0,
                            frame: session::movement::Frame {
                                entity: 42,
                                position: None,
                                rotation: None,
                                on_ground: true,
                                horizontal_collision: false,
                                status_changed: false,
                                input: None,
                                sprinting: None,
                            },
                        })
                        .await
                        .unwrap();
                    commands
                        .send(Command::Select {
                            dimension_epoch: 1,
                            sequence: 1,
                            slot: 0,
                        })
                        .await
                        .unwrap();
                }
                Event::SelectionSent { sequence: 1, .. } => {
                    verified = true;
                    break;
                }
                _ => {}
            }
        }
        cancel.cancel();
        server.await.unwrap();
        let _ = client.await.unwrap();
        assert!(verified);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn native_grouped_cooldowns_decode_signed_removal_and_preserve_following_packets() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            for ticks in [40, 5, 0, -1] {
                // Native protocol777 cooldown index0x16, Identifier followed by signed VarInt.
                assert_eq!(
                    java_protocol::target::id(
                        java_protocol::target::game::clientbound::NAMES,
                        "ClientboundCooldownPacket"
                    ),
                    0x16
                );
                let mut packet = vec![0x16];
                text("fixture:shared", &mut packet);
                codec::write_varint(ticks, &mut packet);
                wire.write(&packet).await.unwrap();
            }
            let keep = 123i64.to_be_bytes();
            wire.write(&incoming("ClientboundKeepAlivePacket", &keep))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundKeepAlivePacket", &keep)
            );
            stop.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let client = tokio::spawn(session::run(
            Options::fixture(address, "Fixture".into(), 7),
            tx,
            cancel.clone(),
        ));
        let mut durations = Vec::new();
        let mut kept_alive = false;
        while let Some(e) = rx.recv().await {
            match e.event {
                Event::Inventory {
                    dimension_epoch: 1,
                    packet: Packet::Cooldown { group, ticks },
                    ..
                } => {
                    assert_eq!(group, "fixture:shared");
                    durations.push(ticks);
                }
                Event::KeepAlive(123) => {
                    kept_alive = true;
                    break;
                }
                _ => {}
            }
        }
        cancel.cancel();
        server.await.unwrap();
        let _ = client.await.unwrap();
        assert!(kept_alive);
        assert_eq!(durations, [40, 5, 0, -1]);
    })
    .await
    .unwrap();
}
