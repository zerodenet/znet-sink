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
    assert!(!status_at(u64::MAX).detailed);
    assert_eq!(CAPTURE_DURATION_MS, 300_000);
    // Exercise a separate atomic-free lease boundary; no process-global test mutation.
    let until = CAPTURE_UNTIL.load(Ordering::Relaxed);
    assert!(!status_at(until).detailed);
}

#[test]
fn event_identity_survives_a_payload_first_in_wire_order() {
    let input: Value = serde_json::from_str(&format!(r#"{{"payload":{{"record":{{"data":"{}"}}}},"event_type":"flow.completed","id":"event-1"}}"#, "x".repeat(100_000))).unwrap();
    let output = project(&input, false, true);
    assert_eq!(output["event_type"], "flow.completed");
    assert_eq!(output["id"], "event-1");
}
