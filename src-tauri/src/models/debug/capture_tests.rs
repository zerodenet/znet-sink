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
