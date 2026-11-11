//! Independent protocol777 configuration/play identities, including encrypted stream reuse.
use super::*;

async fn registries(wire: &mut Stream<tokio::net::TcpStream>, min_y: i32) {
    let mut registry = vec![7];
    text("minecraft:dimension_type", &mut registry);
    registry.push(1);
    text("minecraft:overworld", &mut registry);
    registry.extend([1, 10]);
    for (key, value) in [("min_y", min_y), ("height", 384)] {
        registry.push(3);
        registry.extend((key.len() as u16).to_be_bytes());
        registry.extend(key.bytes());
        registry.extend(value.to_be_bytes());
    }
    registry.push(0);
    wire.write(&registry).await.unwrap();
    let mut biome = vec![7];
    text("minecraft:worldgen/biome", &mut biome);
    biome.push(1);
    text("minecraft:plains", &mut biome);
    biome.extend([1, 10, 0]);
    wire.write(&biome).await.unwrap();
}

#[tokio::test]
async fn reconfiguration_reuses_encryption_cookies_identity_and_retires_old_input() {
    for encrypted in [false, true] {
        tokio::time::timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string().parse().unwrap();
            let cancel = CancellationToken::new();
            let child = cancel.clone();
            let server = tokio::spawn(async move {
                let mut wire = through_configuration(listener, encrypted).await;
                wire.write(&[3]).await.unwrap();
                assert_eq!(wire.read().await.unwrap(), [3]);
                for epoch in 1..=3 {
                    wire.write(&world_login()).await.unwrap();
                    let mut position = vec![0x49, 7];
                    for n in [1f64, 64., 2., 0., 0., 0.] { position.extend(n.to_be_bytes()); }
                    position.extend([0; 12]);
                    wire.write(&position).await.unwrap();
                    let mut tags = vec![0x89, 1, 1]; // play UpdateTags, one registry
                    text("minecraft:block", &mut tags);
                    tags.push(1);
                    text("fixture:retained", &mut tags);
                    tags.extend([1, epoch as u8]);
                    wire.write(&tags).await.unwrap();
                    let mut ack = vec![0, 7];
                    for n in [1f64, 64., 2.] { ack.extend(n.to_be_bytes()); }
                    ack.extend([0; 8]);
                    assert_eq!(wire.read().await.unwrap(), ack);
                    assert_eq!(wire.read().await.unwrap(), [0x2c]);
                    if epoch == 3 { break; }
                    let mut store = vec![0x7a];
                    text("fixture:handoff", &mut store);
                    store.extend([2, 41, 42]);
                    wire.write(&store).await.unwrap();
                    wire.write(&[0x78]).await.unwrap();
                    assert_eq!(wire.read().await.unwrap(), [0x10]);
                    // Reconfiguration sends configuration ClientInformation, never a new handshake/login.
                    assert_eq!(wire.read().await.unwrap(), b"\0\x05en_us\x02\x00\x01\x7f\x01\x00\x00\x00");
                    let mut request = vec![0];
                    text("fixture:handoff", &mut request);
                    wire.write(&request).await.unwrap();
                    let mut response = vec![1];
                    text("fixture:handoff", &mut response);
                    response.extend([1, 2, 41, 42]);
                    assert_eq!(wire.read().await.unwrap(), response);
                    if epoch == 1 {
                        registries(&mut wire, -48).await;
                    } // The next round trip deliberately sends no registry contents.

                    wire.write(&[3]).await.unwrap();
                    assert_eq!(wire.read().await.unwrap(), [3]);
                }
                let mut keep = vec![0x2d]; keep.extend(321i64.to_be_bytes());
                wire.write(&keep).await.unwrap();
                let mut response = vec![0x1c]; response.extend(321i64.to_be_bytes());
                assert_eq!(wire.read().await.unwrap(), response);
                child.cancelled().await;
            });
            let (tx, mut rx) = mpsc::channel(32);
            let (commands, input) = mpsc::channel(16);
            let mut options = Options::fixture(address, "Fixture".into(), 7);
            options.commands = Some(input);
            let client = tokio::spawn(session::run(options, tx, cancel.clone()));
            let (mut configurations, mut bootstraps, mut corrections) = (0, 0, 0);
            let mut identity = None;
            let mut retired_abilities = 0;
            while let Some(envelope) = rx.recv().await {
                assert_eq!(envelope.generation, 7);
                match envelope.event {
                    Event::Phase(Phase::Configuration) => {
                        configurations += 1;
                        if configurations > 1 {
                            commands.send(session::Command::Abilities {
                                dimension_epoch: configurations - 1, authority_revision: 0, flying: true,
                            }).await.unwrap();
                            commands.send(session::Command::Select {
                                dimension_epoch: configurations - 1, sequence: 99, slot: 8,
                            }).await.unwrap();
                        }
                    }
                    Event::Bootstrap(b) => {
                        bootstraps += 1;
                        let same = (b.profile.id, b.server_session);
                        assert_eq!(*identity.get_or_insert(same), same);
                        if bootstraps == 3 {
                            assert_eq!(b.configuration.tags["minecraft:block"]["fixture:retained"], [2]);
                        }
                        let expected = if bootstraps == 1 { -64 } else { -48 };
                        let entry = &b.configuration.registries["minecraft:dimension_type"][0];
                        let java_protocol::nbt::Value::Compound(fields) = &entry.value else { panic!() };
                        assert!(matches!(fields["min_y"], java_protocol::nbt::Value::Int(n) if n == expected));
                    }
                    Event::Teleport { dimension_epoch, sequence, correction } => {
                        corrections += 1;
                        assert_eq!((dimension_epoch, sequence), (corrections, corrections));
                        // A queued receipt/selection from the retired world must never reach the wire.
                        if dimension_epoch > 1 {
                            commands.send(session::Command::Select {
                                dimension_epoch: dimension_epoch - 1, sequence: 100, slot: 8,
                            }).await.unwrap();
                        }
                        commands.send(session::Command::AcceptTeleport {
                            dimension_epoch, sequence, id: correction.id, position: [1.,64.,2.], yaw:0., pitch:0.,
                        }).await.unwrap();
                        commands.send(session::Command::PlayerLoaded { dimension_epoch }).await.unwrap();
                    }
                    Event::AbilitiesWritten(receipt) => {
                        assert!(!receipt.written, "retired play input must not become configuration bytes");
                        retired_abilities += 1;
                    }
                    Event::KeepAlive(321) => break,
                    Event::Phase(_) | Event::Tags(_) => {},
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert_eq!((configurations, bootstraps, corrections), (3, 3, 3));
            assert_eq!(retired_abilities, 2);
            cancel.cancel();
            assert!(matches!(client.await.unwrap(), Err(session::Error::Cancelled)));
            server.await.unwrap();
        }).await.unwrap();
    }
}
