use super::*;
fn recorder() -> Arc<Recorder> {
    Recorder::configured(Some("1")).unwrap()
}
fn count(t: &Recorder, c: Counter) -> u64 {
    t.snapshot()["counts"][c.key()]["count"].as_u64().unwrap()
}
#[test]
fn disabled_by_default_and_file_opt_in_is_bounded_and_exact() {
    for v in [None, Some("0"), Some("false"), Some("garbage")] {
        assert!(Recorder::configured(v).is_none());
    }
    let dir = tempfile::tempdir().unwrap();
    assert!(Recorder::from_directory(dir.path()).is_none());
    let path = dir.path().join("java-telemetry.enabled");
    for data in [b"0".as_slice(), b"1 enormous untrusted file", b"true"] {
        std::fs::write(&path, data).unwrap();
        assert!(Recorder::from_directory(dir.path()).is_none());
    }
    std::fs::write(path, b"1\n").unwrap();
    assert!(Recorder::from_directory(dir.path()).is_some());
}
#[test]
fn every_counter_and_timing_has_real_checklist_ids() {
    let status = include_str!("../../../../../STATUS.md");
    for c in Counter::ALL {
        assert!(!c.items().is_empty());
        for id in c.items() {
            assert!(status.contains(&format!("| {id} |")), "{}: {id}", c.key());
        }
    }
    for s in Stage::ALL {
        for id in s.items() {
            assert!(status.contains(&format!("| {id} |")), "{id}");
        }
    }
}
#[test]
fn corrections_retain_context_and_bound_history_without_positions_or_payloads() {
    let t = recorder();
    t.action_state((7, 3), true, true);
    t.command(&Command::Move {
        dimension_epoch: 3,
        sequence: 1,
        frame: java_protocol::session::movement::Frame {
            entity: 1,
            position: Some([100., 64., 200.]),
            rotation: None,
            on_ground: false,
            status_changed: true,
            horizontal_collision: false,
            input: Some(49),
            sprinting: Some(true),
        },
    });
    t.correction(None, true, false);
    for _ in 0..100 {
        t.correction(Some(2.5), false, true);
    }
    assert_eq!(count(&t, Counter::Corrections), 100);
    assert_eq!(count(&t, Counter::InitialPositions), 1);
    let doc = t.snapshot();
    assert_eq!(doc["recent"].as_array().unwrap().len(), RECENT);
    assert_eq!(doc["correction_distance_m"]["total"], 250.);
    let event = &doc["recent"][0];
    assert_eq!(event["input_bits"], 49);
    assert_eq!(event["sprinting"], true);
    assert_eq!(event["using_item"], true);
    assert_eq!(event["mining"], true);
    assert!(event.get("position").is_none());
}
#[test]
fn prediction_zero_is_unobserved_until_real_hooks_are_connected() {
    let t = recorder();
    assert_eq!(t.snapshot()["coverage"]["local_block_prediction"], false);
    t.prediction(Prediction::Applied, 3);
    t.prediction(Prediction::Confirmed, 2);
    t.prediction(Prediction::Reverted, 1);
    assert_eq!(t.snapshot()["coverage"]["local_block_prediction"], true);
    assert_eq!(count(&t, Counter::RevertedBlocks), 1);
}
#[test]
fn intentional_hiding_and_unavailable_publication_are_distinct() {
    let t = recorder();
    t.hand_frame(true, true, false, HandIssue::Publication);
    t.hand_frame(true, false, true, HandIssue::Publication);
    assert_eq!(count(&t, Counter::AnimationHiddenFrames), 1);
    assert_eq!(count(&t, Counter::AnimationLostPublication), 0);
    t.hand_frame(true, true, false, HandIssue::Publication);
    t.hand_frame(true, false, false, HandIssue::Publication);
    t.hand_frame(true, false, false, HandIssue::Publication);
    assert_eq!(count(&t, Counter::AnimationUnavailableFrames), 2);
    assert_eq!(count(&t, Counter::AnimationLostPublication), 1);
    t.hand_frame(false, false, false, HandIssue::Publication);
    assert_eq!(count(&t, Counter::AnimationUnavailableFrames), 2);
}
#[test]
fn timing_is_scoped_and_missing_samples_are_not_passes() {
    let t = recorder();
    t.loading([true, false, true]);
    t.measure(Stage::TerrainPublication, Duration::from_millis(49));
    t.measure(Stage::TerrainPublication, SPIKE);
    let doc = t.snapshot();
    let timing = &doc["timings"][Stage::TerrainPublication.key()];
    assert_eq!(timing["samples"], 2);
    assert_eq!(timing["spikes"], 1);
    assert_eq!(timing["max_us"], 50000);
    assert_eq!(
        doc["timings"][Stage::EntitySkinPublication.key()]["samples"],
        0
    );
    assert_eq!(
        doc["recent_spikes"][0]["pending_chunks_skins_textures"],
        json!([true, false, true])
    );
}
#[test]
fn retirement_is_fenced_and_other_sessions_are_isolated() {
    let t = recorder();
    t.action_state((7, 1), true, false);
    t.action_state((7, 2), false, false);
    assert_eq!(count(&t, Counter::UseBoundaryCancellation), 1);
    t.count(Counter::ContainerStale, 1);
    let finished = t.finish();
    t.count(Counter::ContainerStale, 99);
    t.measure(Stage::FrameInterval, Duration::from_secs(20));
    assert_eq!(finished, t.snapshot());
    assert_eq!(count(&recorder(), Counter::ContainerStale), 0);
}
#[tokio::test]
async fn bounded_queue_admission_is_unchanged_and_failed_commands_are_not_counted_queued() {
    use crate::java::Session;
    for enabled in [false, true] {
        let mut session = Session::run(|tx, rx| async move {
            let _owners = (tx, rx);
            std::future::pending().await
        });
        session.telemetry = if enabled { Some(recorder()) } else { None };
        for _ in 0..32 {
            assert!(
                session
                    .command(Command::Punch {
                        dimension_epoch: 1,
                        correction_sequence: 0
                    })
                    .is_ok()
            );
        }
        assert!(matches!(
            session.command(Command::Punch {
                dimension_epoch: 1,
                correction_sequence: 0
            }),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
        ));
        if let Some(t) = session.telemetry() {
            assert_eq!(count(t, Counter::SwingAttempts), 32);
            assert_eq!(count(t, Counter::ActionQueueFull), 1);
        }
    }
}
#[test]
fn enabled_session_summary_uses_existing_private_report_and_omits_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let t = recorder();
    t.count(Counter::ContainerStale, 1);
    let mut report = super::super::report::Report::with_telemetry(Some(t));
    report.directory(dir.path().to_owned());
    report.finish("fixture_closed", None);
    let value: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("java-join-latest.json")).unwrap())
            .unwrap();
    assert_eq!(
        value["telemetry"]["counts"]["container_input_stale"]["count"],
        1
    );
    assert_eq!(value["telemetry"]["finished"], true);
    let off = tempfile::tempdir().unwrap();
    let mut report = super::super::report::Report::default();
    report.directory(off.path().to_owned());
    report.finish("fixture_closed", None);
    let value: Value =
        serde_json::from_slice(&std::fs::read(off.path().join("java-join-latest.json")).unwrap())
            .unwrap();
    assert!(value.get("telemetry").is_none());
}

#[test]
fn lost_hand_stage_is_bounded_and_does_not_change_anomaly_classification() {
    let t = recorder();
    t.hand_frame(true, true, false, HandIssue::Publication);
    t.hand_frame(true, false, false, HandIssue::Loading);
    assert_eq!(count(&t, Counter::AnimationLostPublication), 1);
    assert_eq!(count(&t, Counter::AnimationUnavailableFrames), 1);
    let snapshot = t.snapshot();
    assert_eq!(
        snapshot["recent_hand_issues"][0],
        json!({"kind":"hand_publication_lost", "cause":"world_not_ready"})
    );
    t.hand_frame(true, true, false, HandIssue::Publication);
    t.hand_frame(true, false, true, HandIssue::Visibility);
    assert_eq!(count(&t, Counter::AnimationLostPublication), 1);
    assert_eq!(
        t.snapshot()["recent_hand_issues"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn hand_loss_history_is_bounded_without_evicting_correction_context() {
    let t = recorder();
    t.correction(Some(2.5), false, false);
    for _ in 0..100 {
        t.hand_frame(true, true, false, HandIssue::Publication);
        t.hand_frame(true, false, false, HandIssue::EmptyFrame);
    }
    let snapshot = t.snapshot();
    assert_eq!(
        snapshot["recent_hand_issues"].as_array().unwrap().len(),
        RECENT
    );
    assert_eq!(snapshot["recent"].as_array().unwrap().len(), 1);
    assert_eq!(count(&t, Counter::AnimationLostPublication), 100);
}

#[test]
fn use_stop_history_records_exact_static_branch_context_and_is_bounded() {
    let t = recorder();
    for revision in 0..(RECENT as u64 + 3) {
        t.use_stopped("server_using_flag_cleared", 0, revision, true);
    }
    let snapshot = t.snapshot();
    let recent = snapshot["recent_use_stops"].as_array().unwrap();
    assert_eq!(recent.len(), RECENT);
    assert_eq!(recent[0]["server_use_revision"], 3);
    assert_eq!(recent[0]["reason"], "server_using_flag_cleared");
    assert_eq!(recent[0]["held_known"], true);
}

#[test]
fn local_impulse_history_is_bounded_and_does_not_include_identity_or_position() {
    let t = recorder();
    for _ in 0..RECENT + 2 {
        t.server_impulse([0.4, 0.5, -0.1], true, true, false);
    }
    let value = t.snapshot();
    let list = value["recent_server_impulses"].as_array().unwrap();
    assert_eq!(list.len(), RECENT);
    assert_eq!(list[0]["velocity"], json!([0.4, 0.5, -0.1]));
    assert!(list[0].get("entity").is_none());
    assert!(list[0].get("position").is_none());
    assert_eq!(list[0]["admitted"], true);
}
