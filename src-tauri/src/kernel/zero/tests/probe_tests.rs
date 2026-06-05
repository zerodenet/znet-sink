use serde_json::json;

use crate::kernel::zero::parsing::parse_target_probe;

#[test]
fn target_probe_accepts_diagnostics_probe_target_response() {
    let result = parse_target_probe(
        &json!({
            "target_tag": "server-a",
            "server": "1.2.3.4",
            "port": 443,
            "reachable": true,
            "latency_ms": 32
        }),
        "fallback".to_string(),
    );

    assert_eq!(result.target_tag, "server-a");
    assert!(result.reachable);
    assert_eq!(result.latency_ms, Some(32));
    assert_eq!(result.server.as_deref(), Some("1.2.3.4"));
    assert_eq!(result.port, Some(443));
}

#[test]
fn target_probe_handles_unreachable() {
    let result = parse_target_probe(
        &json!({
            "target_tag": "dead",
            "reachable": false,
            "message": "connection refused"
        }),
        "dead".to_string(),
    );

    assert!(!result.reachable);
    assert!(result.latency_ms.is_none());
    assert_eq!(result.message.as_deref(), Some("connection refused"));
}

#[test]
fn target_probe_uses_fallback_tag_when_missing() {
    let result = parse_target_probe(
        &json!({ "reachable": true }),
        "fallback-tag".to_string(),
    );

    assert_eq!(result.target_tag, "fallback-tag");
}

#[test]
fn target_probe_accepts_latency_ms_field() {
    let result = parse_target_probe(
        &json!({
            "target_tag": "x",
            "reachable": true,
            "latency_ms": 50
        }),
        "x".to_string(),
    );

    assert_eq!(result.latency_ms, Some(50));
}

#[test]
fn target_probe_accepts_delay_ms_field() {
    let result = parse_target_probe(
        &json!({
            "target_tag": "x",
            "reachable": true,
            "delay_ms": 75
        }),
        "x".to_string(),
    );

    assert_eq!(result.latency_ms, Some(75));
}
