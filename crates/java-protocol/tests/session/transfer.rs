//! Native encrypted wire fixtures, not vanilla/Hypixel acceptance.
use super::*;
use java_protocol::{session::Command, target, wire::Reader};

fn packet(names: &[&str], name: &str) -> Vec<u8> {
    let mut b = Vec::new();
    codec::write_varint(target::id(names, name), &mut b);
    b
}
fn transfer(names: &[&str], host: &str, port: u16) -> Vec<u8> {
    let mut b = packet(names, "ClientboundTransferPacket");
    text(host, &mut b);
    codec::write_varint(i32::from(port), &mut b);
    b
}
fn loaded() -> Vec<u8> {
    packet(
        target::game::serverbound::NAMES,
        "ServerboundPlayerLoadedPacket",
    )
}
#[tokio::test]
async fn encrypted_transfer_retains_cookies_and_profile_with_new_epoch_and_transfer_intent() {
    tokio::time::timeout(Duration::from_secs(12), async {
        let source = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let from = source.local_addr().unwrap();
        let to = destination.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let first = tokio::spawn(async move {
            let mut wire = through_configuration(source, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), loaded());
            let mut cookie = packet(
                target::game::clientbound::NAMES,
                "ClientboundStoreCookiePacket",
            );
            text("fixture:handoff", &mut cookie);
            codec::write_varint(4, &mut cookie);
            cookie.extend([5, 8, 13, 21]);
            wire.write(&cookie).await.unwrap();
            wire.write(&transfer(
                target::game::clientbound::NAMES,
                "127.0.0.1",
                to.port(),
            ))
            .await
            .unwrap();
            assert!(
                wire.read().await.is_err(),
                "old socket must close before next login"
            );
        });
        let second = tokio::spawn(async move {
            let mut wire = through_configuration_intent(destination, true, 3).await;
            let mut ask = packet(
                target::configuration::clientbound::NAMES,
                "ClientboundCookieRequestPacket",
            );
            text("fixture:handoff", &mut ask);
            wire.write(&ask).await.unwrap();
            let bytes = wire.read().await.unwrap();
            let mut r = Reader(&bytes);
            assert_eq!(
                r.var().unwrap(),
                target::id(
                    target::configuration::serverbound::NAMES,
                    "ServerboundCookieResponsePacket"
                )
            );
            assert_eq!(r.string(32767).unwrap(), "fixture:handoff");
            assert!(r.bool().unwrap());
            assert_eq!(r.bytes(5120).unwrap(), [5, 8, 13, 21]);
            r.finish().unwrap();
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            let mut health = packet(
                target::game::clientbound::NAMES,
                "ClientboundSetHealthPacket",
            );
            health.extend(0f32.to_be_bytes());
            codec::write_varint(20, &mut health);
            health.extend(0f32.to_be_bytes());
            wire.write(&health).await.unwrap();
            // A queued respawn from the retired world must not precede this load receipt.
            assert_eq!(wire.read().await.unwrap(), loaded());
            let mut respawn = packet(
                target::game::serverbound::NAMES,
                "ServerboundClientCommandPacket",
            );
            codec::write_varint(0, &mut respawn);
            assert_eq!(wire.read().await.unwrap(), respawn);
            let mut keep = packet(
                target::game::clientbound::NAMES,
                "ClientboundKeepAlivePacket",
            );
            keep.extend(0x5452414e53464552i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            let mut answer = packet(
                target::game::serverbound::NAMES,
                "ServerboundKeepAlivePacket",
            );
            answer.extend(0x5452414e53464552i64.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), answer);
            child.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(8);
        let mut options = Options::fixture(from.to_string().parse().unwrap(), "Fixture".into(), 7);
        options.commands = Some(input);
        let stop = cancel.clone();
        let client = tokio::spawn(session::run(options, tx, stop));
        let mut bootstraps = 0;
        let mut transfers = 0;
        while let Some(event) = rx.recv().await {
            assert_eq!(event.generation, 7);
            match event.event {
                Event::Bootstrap(_) => {
                    bootstraps += 1;
                    if bootstraps == 2 {
                        commands
                            .send(Command::PlayerLoaded { dimension_epoch: 1 })
                            .await
                            .unwrap();
                    }
                    if bootstraps == 1 {
                        commands
                            .send(Command::PlayerLoaded { dimension_epoch: 1 })
                            .await
                            .unwrap();
                    }
                }
                Event::Vitals(vitals) if vitals.health == 0. && bootstraps == 2 => {
                    commands
                        .send(Command::Respawn { dimension_epoch: 1 })
                        .await
                        .unwrap();
                    commands
                        .send(Command::PlayerLoaded { dimension_epoch: 2 })
                        .await
                        .unwrap();
                    commands
                        .send(Command::Respawn { dimension_epoch: 2 })
                        .await
                        .unwrap();
                }
                Event::ServerTransfer { address } => {
                    assert_eq!(address, to.to_string());
                    transfers += 1;
                }
                Event::KeepAlive(0x5452414e53464552) => break,
                _ => {}
            }
        }
        assert_eq!((bootstraps, transfers), (2, 1));
        cancel.cancel();
        assert!(matches!(
            client.await.unwrap(),
            Err(session::Error::Cancelled)
        ));
        first.await.unwrap();
        second.await.unwrap();
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn offline_transfer_cannot_escape_loopback() {
    let source = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = source.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut wire = through_configuration(source, false).await;
        wire.write(&transfer(
            target::configuration::clientbound::NAMES,
            "192.0.2.1",
            25565,
        ))
        .await
        .unwrap();
        assert!(wire.read().await.is_err());
    });
    let (tx, mut rx) = mpsc::channel(32);
    let client = tokio::spawn(session::run(
        Options::fixture(address.to_string().parse().unwrap(), "Fixture".into(), 7),
        tx,
        Default::default(),
    ));
    while rx.recv().await.is_some() {}
    let result = client.await.unwrap().unwrap_err().to_string();
    assert!(result.contains("restricted to loopback"), "{result}");
    server.await.unwrap();
}

#[tokio::test]
async fn configuration_redirect_chain_is_bounded_without_opening_a_ninth_destination() {
    tokio::time::timeout(Duration::from_secs(12), async {
        let mut listeners = Vec::new();
        for _ in 0..=session::MAX_SERVER_TRANSFERS {
            listeners.push(TcpListener::bind("127.0.0.1:0").await.unwrap());
        }
        let addresses: Vec<_> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
        let first = addresses[0];
        let mut servers = Vec::new();
        for (index, listener) in listeners.into_iter().enumerate() {
            let next = addresses[(index + 1) % addresses.len()];
            servers.push(tokio::spawn(async move {
                let mut wire =
                    through_configuration_intent(listener, false, if index == 0 { 2 } else { 3 })
                        .await;
                wire.write(&transfer(
                    target::configuration::clientbound::NAMES,
                    "127.0.0.1",
                    next.port(),
                ))
                .await
                .unwrap();
                assert!(wire.read().await.is_err());
            }));
        }
        let (tx, mut rx) = mpsc::channel(32);
        let client = tokio::spawn(session::run(
            Options::fixture(first.to_string().parse().unwrap(), "Fixture".into(), 7),
            tx,
            Default::default(),
        ));
        let mut followed = 0;
        while let Some(event) = rx.recv().await {
            if matches!(event.event, Event::ServerTransfer { .. }) {
                followed += 1;
            }
        }
        assert_eq!(followed, session::MAX_SERVER_TRANSFERS);
        assert!(
            client
                .await
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("transfer chain exceeds connection budget")
        );
        for server in servers {
            server.await.unwrap();
        }
    })
    .await
    .unwrap();
}
