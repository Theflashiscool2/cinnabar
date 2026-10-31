//! Protocol-777 tag updates over a compressed TCP fixture, not public-server acceptance.
use super::*;

fn packet(play: bool, registry: &str, entries: &[(&str, &[i32])]) -> Vec<u8> {
    // Pinned MinecraftCodec: configuration ID 14, play ID 137 (two-byte VarInt).
    let mut bytes = if play { vec![0x89, 1, 1] } else { vec![14, 1] };
    text(registry, &mut bytes);
    codec::write_varint(entries.len() as i32, &mut bytes);
    for (name, ids) in entries {
        text(name, &mut bytes);
        codec::write_varint(ids.len() as i32, &mut bytes);
        for id in *ids {
            codec::write_varint(*id, &mut bytes);
        }
    }
    bytes
}

#[tokio::test]
async fn tags_before_and_after_bootstrap_preserve_connection_order() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let server_cancel = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, false).await;
            wire.write(&packet(false, "minecraft:block", &[("fixture:old", &[1])]))
                .await
                .unwrap();
            wire.write(&packet(false, "minecraft:item", &[("fixture:kept", &[2])]))
                .await
                .unwrap();
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            // A tag update in play may precede the world-login/bootstrap packet.
            wire.write(&packet(
                true,
                "minecraft:block",
                &[("fixture:new", &[3, 4])],
            ))
            .await
            .unwrap();
            wire.write(&world_login()).await.unwrap();
            wire.write(&packet(true, "minecraft:block", &[("fixture:live", &[5])]))
                .await
                .unwrap();
            // An empty registry removes its tags; an empty packet leaves all registries alone.
            wire.write(&packet(true, "minecraft:block", &[]))
                .await
                .unwrap();
            wire.write(&[0x89, 1, 0]).await.unwrap();
            let mut keepalive = vec![0x2d];
            keepalive.extend(123i64.to_be_bytes());
            wire.write(&keepalive).await.unwrap();
            let mut expected = vec![0x1c];
            expected.extend(123i64.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), expected); // No tag acknowledgement exists.
            server_cancel.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let client = tokio::spawn(session::run(
            Options::fixture(address, "Fixture".into(), 7),
            tx,
            cancel.clone(),
        ));
        let mut configuration = None;
        let mut updates = 0;
        while let Some(envelope) = rx.recv().await {
            assert_eq!(envelope.generation, 7);
            match envelope.event {
                Event::Bootstrap(b) => {
                    assert_eq!(b.configuration.tags.len(), 2);
                    let block = &b.configuration.tags["minecraft:block"];
                    assert_eq!(block.len(), 1);
                    assert_eq!(block["fixture:new"], [3, 4]);
                    assert_eq!(b.configuration.tags["minecraft:item"]["fixture:kept"], [2]);
                    configuration = Some(b.configuration);
                }
                Event::Tags(tags) => {
                    let state = configuration.as_mut().expect("bootstrap before live tags");
                    state.update_tags(tags).unwrap();
                    updates += 1;
                    if updates == 1 {
                        assert_eq!(state.tags["minecraft:block"].len(), 1);
                        assert_eq!(state.tags["minecraft:block"]["fixture:live"], [5]);
                    } else {
                        assert!(state.tags["minecraft:block"].is_empty());
                    }
                    assert_eq!(state.tags["minecraft:item"]["fixture:kept"], [2]);
                }
                Event::KeepAlive(123) => break,
                Event::Phase(_) => {}
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert_eq!(updates, 3);
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
async fn malformed_tags_never_publish_partial_updates() {
    let valid = packet(true, "minecraft:block", &[("fixture:bad", &[1])]);
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut truncated = valid.clone();
    truncated.pop();
    let negative = packet(true, "minecraft:block", &[("fixture:bad", &[-1])]);
    let duplicate_tag = packet(
        true,
        "minecraft:block",
        &[("fixture:bad", &[1]), ("fixture:bad", &[2])],
    );
    let mut duplicate_registry = valid.clone();
    duplicate_registry[2] = 2;
    duplicate_registry.extend_from_slice(&valid[3..]);
    for bytes in [
        trailing,
        truncated,
        negative,
        duplicate_tag,
        duplicate_registry,
    ] {
        tokio::time::timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string().parse().unwrap();
            let server = tokio::spawn(async move {
                let mut wire = through_configuration(listener, false).await;
                wire.write(&[3]).await.unwrap();
                assert_eq!(wire.read().await.unwrap(), [3]);
                wire.write(&world_login()).await.unwrap();
                wire.write(&bytes).await.unwrap();
                assert!(wire.read().await.is_err());
            });
            let (tx, mut rx) = mpsc::channel(16);
            let result = session::run(
                Options::fixture(address, "Fixture".into(), 7),
                tx,
                CancellationToken::new(),
            )
            .await;
            assert!(
                matches!(result, Err(session::Error::Protocol(_))),
                "{result:?}"
            );
            let mut bootstrap = false;
            while let Some(envelope) = rx.recv().await {
                assert!(!matches!(envelope.event, Event::Tags(_)));
                bootstrap |= matches!(envelope.event, Event::Bootstrap(_));
            }
            assert!(bootstrap);
            server.await.unwrap();
        })
        .await
        .unwrap();
    }
}
