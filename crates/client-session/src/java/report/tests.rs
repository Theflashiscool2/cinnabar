use super::*;
use java_protocol::session::{Phase, entities::metadata};
fn event(event: Event) -> Envelope {
    Envelope {
        generation: 7,
        event,
    }
}

#[test]
fn report_groups_multiple_gaps_and_never_serializes_payloads() {
    let mut report = Report::default();
    report.observe(&event(Event::Phase(Phase::Login)));
    for _ in 0..100 {
        report.observe(&event(Event::DeferredPlay {
            name: "ClientboundBossEventPacket".into(),
            bytes: b"PRIVATE_TOKEN_AND_CHAT".to_vec(),
        }));
    }
    report.observe(&event(Event::DeferredPlay {
        name: "PRIVATE_NAME".into(),
        bytes: vec![],
    }));
    report.observe(&event(Event::Entities {
        dimension_epoch: 1,
        packet: entities::Packet::Metadata(metadata::Packet {
            id: 123456,
            entries: vec![],
            unsupported: Some(metadata::Unsupported {
                index: 2,
                serializer: "ITEM_STACK",
            }),
        }),
    }));
    let mut added = BTreeMap::new();
    added.insert("minecraft:custom_name", json!("PRIVATE_USERNAME"));
    report.observe(&event(Event::Inventory {
        dimension_epoch: 1,
        packet: inventory::Packet::Cursor(Some(inventory::Stack {
            item: 55,
            count: 1,
            added,
            removed: Default::default(),
        })),
    }));
    report.notice("terrain_presentation_failed");
    report.notice("terrain_presentation_failed");
    let document = report.document("error", Some("Java play packet ClientboundContainerSetSlotPacket could not be decoded: PRIVATE_SECRET minecraft:profile"));
    assert_eq!(
        document["deferred_packets"]["ClientboundBossEventPacket"],
        100
    );
    assert_eq!(document["unsupported_entity_serializers"]["ITEM_STACK"], 1);
    assert_eq!(
        document["observed_item_components"],
        json!(["minecraft:custom_name"])
    );
    assert_eq!(
        document["local_notices"],
        json!(["terrain_presentation_failed"])
    );
    assert_eq!(
        document["recent_event_types"].as_array().unwrap().len(),
        RECENT
    );
    assert_eq!(
        document["terminal"]["packet"],
        "ClientboundContainerSetSlotPacket"
    );
    assert_eq!(document["terminal"]["component"], "minecraft:profile");
    let text = document.to_string();
    assert!(!text.contains("PRIVATE"));
    assert!(!text.contains("123456"));
}

#[test]
fn report_rotation_is_bounded_and_completion_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("client.log"), b"preserve me").unwrap();
    for count in 1..=8 {
        let mut report = Report::default();
        report.directory(root.path().into());
        report.events = count;
        report.finish("closed", None);
        report.finish("error", Some("must not overwrite"));
    }
    let read = |name| {
        serde_json::from_slice::<Value>(&std::fs::read(root.path().join(name)).unwrap()).unwrap()
    };
    assert_eq!(read(FILE)["observed_events"], 8);
    assert_eq!(read("java-join-4.json")["observed_events"], 4);
    assert_eq!(
        std::fs::read_dir(root.path()).unwrap().count(),
        RETAINED_REPORTS + 1
    );
    assert_eq!(
        std::fs::read(root.path().join("client.log")).unwrap(),
        b"preserve me"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.path().join(FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn terminal_result_waits_for_pending_events_and_saves_one_attempt() {
    use super::super::Session;
    let root = tempfile::tempdir().unwrap();
    let mut session = Session::run(|tx, _| async move {
        tx.send(event(Event::DeferredPlay {
            name: "ClientboundBossEventPacket".into(),
            bytes: vec![1, 2],
        }))
        .await
        .unwrap();
        tx.send(event(Event::Entities {
            dimension_epoch: 1,
            packet: entities::Packet::Metadata(metadata::Packet {
                id: 1,
                entries: vec![],
                unsupported: Some(metadata::Unsupported {
                    index: 1,
                    serializer: "PARTICLES",
                }),
            }),
        }))
        .await
        .unwrap();
        Err("native Java behavior not implemented PRIVATE_SECRET".into())
    })
    .with_report_directory(root.path().into());
    (&mut session.task).await.unwrap();
    assert!(session.result().is_none());
    assert!(session.poll().is_some());
    assert!(session.result().is_none());
    assert!(session.next().await.is_some());
    assert!(session.result().unwrap().is_err());
    drop(session);
    let document: Value =
        serde_json::from_slice(&std::fs::read(root.path().join(FILE)).unwrap()).unwrap();
    assert_eq!(document["observed_events"], 2);
    assert_eq!(document["unsupported_entity_serializers"]["PARTICLES"], 1);
    assert_eq!(document["outcome"], "error");
    assert!(!document.to_string().contains("PRIVATE_SECRET"));
    assert!(!root.path().join("java-join-1.json").exists());
}

#[tokio::test]
async fn retirement_captures_pending_notices_without_leaking_into_next_attempt() {
    use super::super::Session;
    let root = tempfile::tempdir().unwrap();
    let (ready, wait) = tokio::sync::oneshot::channel();
    let session = Session::run(|tx, _| async move {
        tx.send(event(Event::Phase(Phase::Configuration)))
            .await
            .unwrap();
        let _ = ready.send(());
        std::future::pending().await
    })
    .with_report_directory(root.path().into());
    wait.await.unwrap();
    session.shutdown().await;
    let first: Value =
        serde_json::from_slice(&std::fs::read(root.path().join(FILE)).unwrap()).unwrap();
    assert_eq!(first["phases_reached"], json!(["configuration"]));
    assert_eq!(first["outcome"], "cancelled_or_retired");
    let mut next = Session::prepare(async { Err("account setup failed".into()) })
        .with_report_directory(root.path().into());
    assert!(next.next().await.is_none());
    assert!(next.outcome().await.is_err());
    let last: Value =
        serde_json::from_slice(&std::fs::read(root.path().join(FILE)).unwrap()).unwrap();
    assert_eq!(last["observed_events"], 0);
    assert_eq!(last["phases_reached"], json!([]));
    assert_eq!(last["terminal"]["category"], "authentication_or_account");
}

#[tokio::test]
async fn admission_failure_keeps_its_reason_and_pending_evidence() {
    use super::super::Session;
    let root = tempfile::tempdir().unwrap();
    let (ready, wait) = tokio::sync::oneshot::channel();
    let mut session = Session::run(|tx, _| async move {
        tx.send(event(Event::KeepAlive(123))).await.unwrap();
        let _ = ready.send(());
        std::future::pending().await
    })
    .with_report_directory(root.path().into());
    wait.await.unwrap();
    session.fail_admission("invalid native inventory identity");
    drop(session);
    let report: Value =
        serde_json::from_slice(&std::fs::read(root.path().join(FILE)).unwrap()).unwrap();
    assert_eq!(report["outcome"], "world_admission_error");
    assert_eq!(report["observed_events"], 1);
    assert_eq!(
        report["local_notices"],
        json!(["invalid native inventory identity"])
    );
}

#[tokio::test]
async fn unwritable_report_directory_does_not_replace_the_connection_error() {
    use super::super::Session;
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, b"preserve").unwrap();
    let mut session = Session::prepare(async { Err("original connection failure".into()) })
        .with_report_directory(file.clone());
    assert!(session.next().await.is_none());
    assert_eq!(
        session.outcome().await.unwrap_err(),
        "original connection failure"
    );
    assert_eq!(std::fs::read(file).unwrap(), b"preserve");
}

#[test]
fn unknown_server_names_and_repeated_local_notices_cannot_grow_the_report() {
    let mut report = Report::default();
    for name in target::game::clientbound::NAMES {
        report.notice(name);
        report.observe(&event(Event::DeferredPlay {
            name: format!("private-untrusted-{name}"),
            bytes: vec![],
        }));
    }
    assert_eq!(report.notices.len(), NOTICES);
    assert!(report.discarded_notices > 0);
    assert_eq!(report.deferred.len(), 1);
    assert_eq!(report.recent.len(), RECENT);
    assert!(
        !report
            .document("closed", None)
            .to_string()
            .contains("private-untrusted")
    );
}
