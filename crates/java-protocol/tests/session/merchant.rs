//! Valid catalog admission and scoped refusal are not trading or Hypixel acceptance.
use super::containers::{incoming, outgoing};
use super::*;
use session::{Command, inventory::Packet};
#[tokio::test]
async fn merchant_refusal_preserves_connection_and_reused_chest_menu() {
    const KEEP: i64 = 0x4d45524348414e54;
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
            let kind = java_protocol::target::CONTAINER_TYPES
                .iter()
                .position(|k| *k == "MERCHANT")
                .unwrap();
            let mut open = vec![7];
            codec::write_varint(kind as i32, &mut open);
            open.extend([8, 0, 6]);
            open.extend(b"Trader");
            wire.write(&incoming("ClientboundOpenScreenPacket", &open))
                .await
                .unwrap();
            let hex = include_str!("../../data/merchant-offers-fixture.hex").trim();
            let body = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            wire.write(&incoming("ClientboundMerchantOffersPacket", &body))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundContainerClosePacket", &[7])
            );
            wire.write(&super::containers::open()).await.unwrap();
            let body = KEEP.to_be_bytes();
            wire.write(&incoming("ClientboundKeepAlivePacket", &body))
                .await
                .unwrap();
            assert_eq!(
                wire.read().await.unwrap(),
                outgoing("ServerboundKeepAlivePacket", &body)
            );
            stop.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let (commands, input) = mpsc::channel(8);
        let mut options = Options::fixture(address, "Fixture".into(), 7);
        options.commands = Some(input);
        let client = tokio::spawn(session::run(options, tx, cancel.clone()));
        let (mut offers, mut retired, mut keep) = (false, false, false);
        while let Some(e) = rx.recv().await {
            match e.event {
                Event::Bootstrap(_) => commands
                    .send(Command::PlayerLoaded { dimension_epoch: 1 })
                    .await
                    .unwrap(),
                Event::Inventory {
                    incarnation,
                    packet: Packet::MerchantOffers(summary),
                    ..
                } => {
                    assert_eq!((summary.container, summary.offers), (7, 1));
                    offers = true;
                    commands
                        .send(Command::ContainerClose {
                            dimension_epoch: 1,
                            incarnation,
                            sequence: 1,
                            container: 7,
                        })
                        .await
                        .unwrap();
                }
                Event::Inventory {
                    incarnation,
                    packet:
                        Packet::Open {
                            kind: "GENERIC_9X3",
                            ..
                        },
                    ..
                } => {
                    assert_eq!(incarnation, 2);
                    commands
                        .send(Command::ContainerClose {
                            dimension_epoch: 1,
                            incarnation: 1,
                            sequence: 2,
                            container: 7,
                        })
                        .await
                        .unwrap();
                }
                Event::InventorySent {
                    sequence: 2,
                    written: false,
                    ..
                } => retired = true,
                Event::KeepAlive(id) if id == KEEP => keep = true,
                _ => {}
            }
            if offers && retired && keep {
                cancel.cancel();
                break;
            }
        }
        assert!(offers && retired && keep);
        assert!(matches!(
            client.await.unwrap(),
            Err(session::Error::Cancelled)
        ));
        server.await.unwrap();
    })
    .await
    .unwrap();
}
