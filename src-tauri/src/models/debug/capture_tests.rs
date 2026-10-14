use super::*;
use serde_json::json;

#[test]
fn large_stats_are_projected_before_copying_and_have_hard_budgets() {
    let input = json!({"id": "stats", "ok": true, "data": {
        "items": (0..5000).map(|id| json!({"id": id, "message": "x".repeat(4096)})).collect::<Vec<_>>()
    }});
    let summary = project(&input, false, false);
    let detail = project(&input, true, false);
    assert!(serde_json::to_vec(&summary).unwrap().len() < 2304);
    assert!(serde_json::to_vec(&detail).unwrap().len() < 17 * 1024);
    assert_eq!(detail["_diagnosticCapture"]["truncated"], true);
    assert_eq!(summary["id"], "stats");
    assert_eq!(summary["ok"], true);
}

#[test]
fn projection_redacts_config_and_credentials_even_in_detail_mode() {
    let input = json!({"params": {"config": {"value": "canary-config"}, "password": "canary-secret"},
        "nested": [{"token": "canary-token", "url": "https://private.example/key"}]});
    let output = project(&input, true, false).to_string();
    assert!(!output.contains("canary"));
    assert!(!output.contains("private.example"));
}

#[test]
fn connection_history_keeps_its_record_without_detailed_capture() {
    let input = json!({"event_type": "flow.completed", "payload": {"record": {
        "network": "tcp", "path": {"outbound": {"tag": "company"}},
        "target": {"host": "example.test", "port": 443}, "bytes_received": 1234
    }}});
    let output = project(&input, false, true);
    assert_eq!(output["payload"]["record"], input["payload"]["record"]);
    assert_eq!(output["_diagnosticCapture"]["mode"], "connection-history");
}

#[test]
fn pathological_keys_unicode_and_depth_stay_bounded() {
    let input = json!({"long": "😀\n\"".repeat(10000), "nested": [[[[[[[[[[[[[[42]]]]]]]]]]]]]]});
    for (detailed, completed, limit) in [
        (false, false, 2304),
        (true, false, 17 * 1024),
        (false, true, 33 * 1024),
    ] {
        assert!(project(&input, detailed, completed).to_string().len() < limit);
    }
    let huge_key = json!({"x".repeat(10000): "y"});
    assert!(project(&huge_key, true, false).to_string().len() < 300);
}

#[test]
fn capture_is_off_by_default_and_expires_without_a_timer() {
    let mut session = Session::default();
    assert!(!session.view(0).detailed);
    assert_eq!(CAPTURE_DURATION_MS, 300_000);
    session.start(1, 1, None, 10).unwrap();
    let expired = session.view(1010);
    assert!(!expired.detailed);
    assert_eq!(expired.end_reason, Some("duration"));
    assert_eq!(expired.ended_at_ms, Some(1010));
}

#[test]
fn event_identity_survives_a_payload_first_in_wire_order() {
    let input: Value = serde_json::from_str(&format!(r#"{{"payload":{{"record":{{"data":"{}"}}}},"event_type":"flow.completed","id":"event-1"}}"#, "x".repeat(100_000))).unwrap();
    let output = project(&input, false, true);
    assert_eq!(output["event_type"], "flow.completed");
    assert_eq!(output["id"], "event-1");
}

#[test]
fn session_capacity_counts_written_records_and_never_exceeds_total_budget() {
    let mut session = Session::default();
    let status = session.start(300, 1, None, 0).unwrap();
    let id = status.session_id.unwrap();
    assert!(session.admit(&id, MIB - 100, 1));
    session.written(&id, MIB - 100, true, 2);
    assert!(!session.admit(&id, 101, 3));
    let stopped = session.view(3);
    assert_eq!(stopped.end_reason, Some("capacity"));
    assert_eq!(stopped.stored_bytes, MIB - 100);
    let next = session.start(1, 1, None, 4).unwrap();
    assert!(!session.admit(&id, 1, 5));
    assert_eq!(session.view(5).session_id, next.session_id);
}
#[test]
fn invalid_budgets_and_storage_errors_never_claim_success() {
    let mut session = Session::default();
    for (duration, capacity) in [(0, 1), (301, 1), (1, 0), (1, 11)] {
        assert!(session.start(duration, capacity, None, 0).is_err());
    }
    let status = session.start(1, 1, None, 0).unwrap();
    let id = status.session_id.unwrap();
    session.written(&id, 100, false, 1);
    let stopped = session.view(1);
    assert_eq!(stopped.stored_bytes, 0);
    assert_eq!(stopped.end_reason, Some("storage_failure"));
}

#[test]
fn old_session_stop_and_changed_source_scope_cannot_control_replacement_capture() {
    use crate::client_core::{ClientScope, ConfigRevision, CoreInstanceId, ProfileId};
    let scope = ClientScope {
        profile_id: Some(ProfileId("p".into())),
        config_revision: ConfigRevision(1),
        core_instance_id: CoreInstanceId(1),
    };
    let mut session = Session::default();
    let original = session.start(300, 1, Some(scope.clone()), 0).unwrap();
    assert!(session.start(300, 1, Some(scope.clone()), 1).is_err());
    let mut changed = scope.clone();
    changed.core_instance_id = CoreInstanceId(2);
    session.invalidate(&changed, 2);
    assert_eq!(session.view(2).end_reason, Some("scope_changed"));
    let next = session.start(300, 1, Some(changed), 3).unwrap();
    assert_ne!(original.session_id, next.session_id);
    assert!(session
        .stop_expected(original.session_id.as_deref(), 4)
        .is_err());
    assert!(session.view(4).detailed);
    session
        .stop_expected(next.session_id.as_deref(), 5)
        .unwrap();
    assert_eq!(session.view(5).end_reason, Some("user"));
}
#[test]
fn monotonic_expiry_survives_a_wall_clock_move_backwards() {
    let mut session = Session::default();
    session.start(1, 1, None, 10_000).unwrap();
    session.deadline = Some(std::time::Instant::now());
    assert!(!session.view(1).detailed);
    assert_eq!(session.view(1).end_reason, Some("duration"));
}

fn journal_session(path: &Path) -> Session {
    let mut session = Session::default();
    session.initialize(path).unwrap();
    session
}
#[test]
fn restart_reads_checkpoint_and_projects_interrupted_without_resume_or_inferred_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let mut first = journal_session(&path);
    let original = first.start(1, 1, None, 10).unwrap();
    let id = original.session_id.unwrap();
    first.written(&id, 100, true, 11);
    // A read neither renews the deadline nor writes a producer-side checkpoint.
    let checkpoint = std::fs::read(&path).unwrap();
    assert!(first.view(12).detailed);
    assert_eq!(std::fs::read(&path).unwrap(), checkpoint);
    let mut restarted = journal_session(&path);
    let recovered = restarted.view(20_000);
    assert!(!recovered.detailed);
    assert_eq!(recovered.state, "interrupted");
    assert_eq!(recovered.end_reason, Some("process_restart"));
    assert!(recovered.ended_at_ms.is_none());
    assert_eq!(recovered.stored_bytes, 100);
    assert_eq!(recovered.stored_frames, 1);
    assert!(recovered.recovered);
    assert_eq!(recovered.recovery_state, "confirmed");
    assert_eq!(
        recovered.recovery_history[0].confirmation,
        CaptureConfirmation::ResultUnknown
    );
    assert!(restarted.stop_expected(Some(&id), 20_001).is_err());
    assert_eq!(restarted.view(20_001).state, "interrupted");
    let explicitly_new = restarted.start(1, 1, None, 20_002).unwrap();
    assert_ne!(explicitly_new.session_id.as_deref(), Some(id.as_str()));
    assert_eq!(explicitly_new.recovery_history.len(), 2);
    assert_eq!(
        explicitly_new.recovery_history[0].confirmation,
        CaptureConfirmation::ResultUnknown
    );
}
#[test]
fn confirmed_stop_history_survives_restart_and_is_bounded_to_32_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let mut first = journal_session(&path);
    for index in 0..35 {
        let started = first.start(300, 1, None, index * 10).unwrap();
        let id = started.session_id.unwrap();
        first.written(&id, 100, true, index * 10 + 1);
        first.stop_expected(Some(&id), index * 10 + 2).unwrap();
    }
    let mut restarted = journal_session(&path);
    let status = restarted.view(1000);
    assert_eq!(status.state, "stopped");
    assert_eq!(status.end_reason, Some("user"));
    assert_eq!(status.recovery_history.len(), 32);
    assert!(status
        .recovery_history
        .iter()
        .all(|record| record.phase == CapturePhase::Stopped
            && record.confirmation == CaptureConfirmation::Confirmed));
    assert_eq!(status.stored_bytes, 100);
    assert!(
        std::fs::metadata(path).unwrap().len()
            <= crate::services::capture_recovery::BYTE_LIMIT as u64
    );
}
#[test]
fn failed_start_does_not_enable_detail_and_corrupt_load_does_not_overwrite_old_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let mut session = journal_session(&path);
    std::fs::create_dir(&path).unwrap();
    assert!(session.start(1, 1, None, 0).is_err());
    let status = session.view(1);
    assert!(!status.detailed);
    assert_eq!(status.state, "interrupted");
    assert_eq!(status.recovery_state, "result_unknown");
    assert_eq!(status.end_reason, Some("journal_failure"));
    assert!(session.start(1, 1, None, 2).is_err());
    let corrupt = dir.path().join("corrupt.json");
    std::fs::write(&corrupt, b"secret-canary").unwrap();
    let mut recovery = Session::default();
    assert!(recovery.initialize(&corrupt).is_err());
    assert!(recovery.start(1, 1, None, 3).is_err());
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"secret-canary");
    assert_eq!(recovery.view(4).recovery_state, "result_unknown");
    recovery.clear_recovery().unwrap();
    assert_eq!(recovery.view(5).state, "idle");
    assert_eq!(recovery.view(5).recovery_state, "confirmed");
    assert!(recovery.start(1, 1, None, 6).is_ok());
}
fn block_parent(parent: &Path, archive: &Path) {
    std::fs::rename(parent, archive).unwrap();
    std::fs::write(parent, b"block-private-parent").unwrap();
}
#[test]
fn counter_checkpoint_failure_stops_detail_keeps_last_receipt_and_does_not_retry() {
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("capture-owner");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("capture-recovery.json");
    let mut session = journal_session(&path);
    let id = session.start(300, 1, None, 0).unwrap().session_id.unwrap();
    let archive = dir.path().join("preserved");
    block_parent(&parent, &archive);
    session.written(&id, 100, true, 1);
    let status = session.view(2);
    assert!(!status.detailed);
    assert_eq!(status.stored_bytes, 100);
    assert_eq!(status.recovery_state, "result_unknown");
    assert_eq!(status.recovery_history[0].stored_bytes, 0);
    assert_eq!(status.end_reason, Some("journal_failure"));
    let checkpoint = std::fs::read(archive.join("capture-recovery.json")).unwrap();
    assert!(session.start(1, 1, None, 3).is_err());
    assert!(session.stop_expected(Some(&id), 4).is_err());
    assert_eq!(
        std::fs::read(archive.join("capture-recovery.json")).unwrap(),
        checkpoint
    );
    assert!(session.clear_recovery().is_err());
    assert_eq!(session.view(5).recovery_history.len(), 1);
    std::fs::remove_file(&parent).unwrap();
    std::fs::rename(&archive, &parent).unwrap();
    session.clear_recovery().unwrap();
    assert!(session.view(6).recovery_history.is_empty());
}
#[test]
fn failed_stop_or_expiry_checkpoint_is_unknown_and_never_confirmed_completed() {
    for automatic in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("owner");
        std::fs::create_dir(&parent).unwrap();
        let mut session = journal_session(&parent.join("capture-recovery.json"));
        let id = session.start(1, 1, None, 0).unwrap().session_id.unwrap();
        block_parent(&parent, &dir.path().join("preserved"));
        if automatic {
            session.view(1000);
        } else {
            assert!(session.stop_expected(Some(&id), 1).is_err());
        }
        let unknown = session.view(1001);
        assert_eq!(unknown.state, "interrupted");
        assert_eq!(unknown.end_reason, Some("journal_failure"));
        assert_eq!(unknown.recovery_state, "result_unknown");
        assert!(unknown.ended_at_ms.is_none());
        assert!(!unknown.detailed);
    }
}
#[test]
fn scope_invalidation_and_storage_failure_have_confirmed_terminal_metadata() {
    use crate::client_core::{ClientScope, ConfigRevision, CoreInstanceId};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let mut session = journal_session(&path);
    let scope = ClientScope {
        profile_id: None,
        config_revision: ConfigRevision(1),
        core_instance_id: CoreInstanceId(1),
    };
    session.start(300, 1, Some(scope.clone()), 0).unwrap();
    session.invalidate(
        &ClientScope {
            core_instance_id: CoreInstanceId(2),
            ..scope
        },
        1,
    );
    assert_eq!(session.view(1).end_reason, Some("scope_changed"));
    let id = session.start(300, 1, None, 2).unwrap().session_id.unwrap();
    session.written(&id, 100, false, 3);
    let mut restarted = journal_session(&path);
    let status = restarted.view(4);
    assert_eq!(status.end_reason, Some("storage_failure"));
    assert_eq!(status.stored_bytes, 0);
    assert_eq!(
        status.recovery_history[0].end_reason,
        Some(CaptureEndReason::ScopeChanged)
    );
    assert_eq!(
        status.recovery_history[1].confirmation,
        CaptureConfirmation::Confirmed
    );
}
#[test]
fn deferred_old_counter_checkpoint_cannot_overwrite_newer_confirmed_stop_or_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let mut session = journal_session(&path);
    let original = session.start(300, 1, None, 0).unwrap().session_id.unwrap();
    let deferred = session
        .written_deferred(&original, 100, true, 1)
        .unwrap()
        .unwrap();
    session.stop_expected(Some(&original), 2).unwrap();
    let replacement = session.start(300, 1, None, 3).unwrap();
    let result = deferred
        .store
        .save_ordered(deferred.sequence, &deferred.records);
    assert!(!*result.as_ref().unwrap());
    session.complete_checkpoint(deferred, result).unwrap();
    let stored = CaptureRecoveryStore::new(&path).load().unwrap();
    assert_eq!(stored[0].phase, CapturePhase::Stopped);
    assert_eq!(stored[0].stored_bytes, 100);
    assert_eq!(stored[1].session_id, replacement.session_id.unwrap());
    assert_eq!(stored[1].phase, CapturePhase::Capturing);
}
#[test]
fn blocked_worker_journal_write_releases_capture_lock_for_healthy_producer_reads() {
    use std::sync::{Arc, Mutex};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let mut initial = journal_session(&path);
    let id = initial.start(300, 1, None, 0).unwrap().session_id.unwrap();
    let store = initial.recovery.clone().unwrap();
    let held = store.hold_publication_for_test();
    let shared = Arc::new(Mutex::new(initial));
    let worker_state = shared.clone();
    let (ready, received) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let checkpoint = worker_state
            .lock()
            .unwrap()
            .written_deferred(&id, 100, true, 1)
            .unwrap()
            .unwrap();
        ready.send(()).unwrap();
        let result = checkpoint
            .store
            .save_ordered(checkpoint.sequence, &checkpoint.records);
        worker_state
            .lock()
            .unwrap()
            .complete_checkpoint(checkpoint, result)
            .unwrap();
    });
    received
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    // Disk is deliberately blocked, but the capture-state mutex is free.
    assert!(shared.try_lock().unwrap().view(2).detailed);
    drop(held);
    worker.join().unwrap();
    assert_eq!(store.load().unwrap()[0].stored_bytes, 100);
}
