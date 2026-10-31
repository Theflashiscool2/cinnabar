//! Batch transport probes; dispatch recognition is distinct from complete gameplay support.
use super::*;
#[path = "../support/item_corpus.rs"]
mod corpus;

#[tokio::test]
async fn all_component_types_survive_ordered_inventory_delivery_and_keepalive() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, true).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            for (index, group) in corpus::fixtures().chunks(46).enumerate() {
                let mut contents = vec![18, 0, (index + 1) as u8, 46];
                for (name, bytes) in group {
                    contents.extend(corpus::item(name, bytes));
                }
                contents.extend(std::iter::repeat_n(0, 46 - group.len()));
                contents.push(0); // empty authoritative cursor
                wire.write(&contents).await.unwrap();
            }
            let mut keep = vec![0x2d];
            keep.extend(123i64.to_be_bytes());
            wire.write(&keep).await.unwrap();
            let mut response = vec![0x1c];
            response.extend(123i64.to_be_bytes());
            assert_eq!(wire.read().await.unwrap(), response);
            child.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let client = tokio::spawn(session::run(
            Options::fixture(address, "Fixture".into(), 7),
            tx,
            cancel.clone(),
        ));
        let mut names = std::collections::BTreeSet::new();
        let mut revisions = Vec::new();
        while let Some(event) = rx.recv().await {
            match event.event {
                Event::Inventory {
                    dimension_epoch,
                    packet:
                        session::inventory::Packet::Content {
                            container,
                            state,
                            slots,
                            carried,
                        },
                } => {
                    assert_eq!((event.generation, dimension_epoch, container), (7, 1, 0));
                    assert_eq!(slots.len(), 46);
                    assert!(carried.is_none());
                    revisions.push(state);
                    for item in slots.into_iter().flatten() {
                        names.extend(item.added.keys().copied());
                    }
                }
                Event::KeepAlive(123) => break,
                Event::Phase(_) | Event::Bootstrap(_) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(revisions, [1, 2, 3]);
        assert_eq!(
            names,
            java_protocol::target::ITEM_COMPONENTS
                .iter()
                .map(|s| s.name)
                .collect()
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
async fn census_all_play_packet_dispatch_gates() {
    let mut unsupported = Vec::new();
    let mut recognized = 0;
    let mut accepted = 0;
    for (id, name) in java_protocol::target::game::clientbound::NAMES
        .iter()
        .enumerate()
    {
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap().to_string().parse().unwrap();
            let cancel = CancellationToken::new();
            let child = cancel.clone();
            let server = tokio::spawn(async move {
                let mut wire = through_configuration(listener, false).await;
                wire.write(&[3]).await.unwrap();
                assert_eq!(wire.read().await.unwrap(), [3]);
                wire.write(&world_login()).await.unwrap();
                let mut packet = Vec::new();
                codec::write_varint(id as i32, &mut packet);
                wire.write(&packet).await.unwrap();
                let mut keep = vec![0x2d];
                keep.extend(123i64.to_be_bytes());
                let _ = wire.write(&keep).await;
                child.cancelled().await;
            });
            let (tx, mut rx) = mpsc::channel(16);
            let client = tokio::spawn(session::run(
                Options::fixture(address, "Fixture".into(), 7),
                tx,
                cancel.clone(),
            ));
            while let Some(event) = rx.recv().await {
                if matches!(event.event, Event::KeepAlive(123)) {
                    cancel.cancel();
                    break;
                }
            }
            let result = client.await.unwrap();
            cancel.cancel();
            server.await.unwrap();
            result
        })
        .await
        .unwrap();
        match result {
            Err(session::Error::Unsupported(reason)) => {
                unsupported.push((name.to_string(), reason.to_string()))
            }
            Err(session::Error::Cancelled) => accepted += 1,
            Err(session::Error::Protocol(_) | session::Error::PlayPacket { .. }) => recognized += 1,
            other => panic!("{name}: unexpected probe result {other:?}"),
        }
    }
    eprintln!(
        "Dispatch census: {accepted} empty/optional accepted; {recognized} handlers rejected incomplete/invalid bodies; explicit gates={unsupported:?}"
    );
    // These demand additional session/UI behavior; keep them explicit instead of faking success.
    let expected = [
        "ClientboundMountScreenOpenPacket",
        "ClientboundMerchantOffersPacket",
        "ClientboundMoveVehiclePacket",
        "ClientboundOpenBookPacket",
        "ClientboundOpenSignEditorPacket",
        "ClientboundPongResponsePacket",
        "ClientboundPlaceGhostRecipePacket",
        "ClientboundPlayerChatPacket",
        "ClientboundStartConfigurationPacket",
        "ClientboundTestInstanceBlockStatus",
        "ClientboundTransferPacket",
        "ClientboundShowDialogGamePacket",
    ];
    assert_eq!(
        unsupported
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        expected.into_iter().collect(),
        "all unsupported gates: {unsupported:?}"
    );
}

#[tokio::test]
async fn malformed_item_reports_packet_context_without_emitting_inventory() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string().parse().unwrap();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let server = tokio::spawn(async move {
            let mut wire = through_configuration(listener, false).await;
            wire.write(&[3]).await.unwrap();
            assert_eq!(wire.read().await.unwrap(), [3]);
            wire.write(&world_login()).await.unwrap();
            let mut contents = vec![18, 0, 1, 1];
            // A declared float with missing bytes must not produce a partial item snapshot.
            contents.extend(corpus::item("minecraft:custom_model_data", &[1, 0]));
            wire.write(&contents).await.unwrap();
            child.cancelled().await;
        });
        let (tx, mut rx) = mpsc::channel(16);
        let client = tokio::spawn(session::run(
            Options::fixture(address, "Fixture".into(), 7),
            tx,
            cancel.clone(),
        ));
        while let Some(event) = rx.recv().await {
            assert!(!matches!(event.event, Event::Inventory { .. }));
        }
        let error = client.await.unwrap().unwrap_err();
        assert!(matches!(
            error,
            session::Error::PlayPacket {
                packet: "ClientboundContainerSetContentPacket",
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .contains("ClientboundContainerSetContentPacket")
        );
        cancel.cancel();
        server.await.unwrap();
    })
    .await
    .unwrap();
}
