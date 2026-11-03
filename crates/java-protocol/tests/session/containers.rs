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
