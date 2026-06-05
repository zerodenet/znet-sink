use serde_json::json;

use crate::kernel::zero::parsing::*;

#[test]
fn parse_stats_accepts_current_zero_fields() {
    let stats = parse_stats(&json!({
        "active_sessions": 2,
        "total_started": 10,
        "completed_sessions": 7,
        "failed_sessions": 1,
        "blocked_sessions": 1,
        "direct_sessions": 3,
        "chained_sessions": 4,
        "bytes_up": 1200,
        "bytes_down": 3400
    }));

    assert_eq!(stats.active_sessions, 2);
    assert_eq!(stats.total_started, 10);
    assert_eq!(stats.bytes_up, 1200);
    assert_eq!(stats.bytes_down, 3400);
}

#[test]
fn parse_health_maps_engine_build_id() {
    let health = parse_health(&json!({
        "engine_build_id": "0.0.9",
        "started_at_unix_ms": 1713500000000_u64,
        "healthy": true
    }));

    assert!(health.healthy);
    assert_eq!(health.engine_version.as_deref(), Some("0.0.9"));
    assert_eq!(health.started_at_unix_ms, Some(1713500000000));
}

#[test]
fn parse_health_strips_v_prefix() {
    let health = parse_health(&json!({
        "engine_build_id": "v0.0.5",
        "healthy": true
    }));

    assert_eq!(health.engine_version.as_deref(), Some("0.0.5"));
}

#[test]
fn parse_health_defaults_healthy_when_missing() {
    let health = parse_health(&json!({}));

    assert!(health.healthy);
    assert!(health.engine_version.is_none());
    assert!(health.started_at_unix_ms.is_none());
}

#[test]
fn parse_capabilities_maps_api_id_field() {
    let caps = parse_capabilities(
        &json!({
            "api_id": "zero.api.v1",
            "schema_id": "zero.event.v1",
            "features": ["query", "config_snapshot"],
            "permissions": ["read"],
            "adapters": [{ "kind": "in_process", "enabled": true }],
            "sinks": []
        }),
        None,
    );

    assert!(caps.available);
    assert_eq!(caps.api_version.as_deref(), Some("zero.api.v1"));
    assert_eq!(caps.schema_version.as_deref(), Some("zero.event.v1"));
    assert_eq!(caps.features, vec!["query", "config_snapshot"]);
    assert_eq!(caps.adapters.len(), 1);
    assert_eq!(caps.adapters[0].kind, "in_process");
    assert!(caps.adapters[0].enabled);
}

#[test]
fn parse_capabilities_captures_error() {
    let caps = parse_capabilities(&json!({}), Some("connection refused".to_string()));

    assert!(!caps.available);
    assert_eq!(caps.error.as_deref(), Some("connection refused"));
}

#[test]
fn parse_connection_accepts_nested_target() {
    let conn = parse_connection(&json!({
        "flow_id": "42",
        "network": "tcp",
        "target": { "host": "example.com", "port": 443 },
        "traffic": { "bytes_up": 100, "bytes_down": 200 },
        "timing": { "started_at_unix_ms": 1713500000000_u64 }
    }))
    .unwrap();

    assert_eq!(conn.flow_id, "42");
    assert_eq!(conn.network, "tcp");
    assert_eq!(conn.destination, "example.com:443");
    assert_eq!(conn.bytes_up, 100);
    assert_eq!(conn.bytes_down, 200);
    assert_eq!(conn.started_at_unix_ms, Some(1713500000000));
}

#[test]
fn parse_connection_accepts_flat_destination() {
    let conn = parse_connection(&json!({
        "flow_id": "1",
        "host": "1.2.3.4",
        "port": 80,
        "inbound_tag": "socks5",
        "outbound_tag": "proxy"
    }))
    .unwrap();

    assert_eq!(conn.destination, "1.2.3.4:80");
    assert_eq!(conn.inbound_tag.as_deref(), Some("socks5"));
    assert_eq!(conn.outbound_tag.as_deref(), Some("proxy"));
}

#[test]
fn parse_connection_returns_none_without_flow_id() {
    assert!(parse_connection(&json!({ "host": "x" })).is_none());
}

#[test]
fn unwrap_core_envelope_strips_ok_result() {
    let value = unwrap_core_envelope(json!({
        "ok": true,
        "result": { "engine_build_id": "0.0.9" }
    }))
    .unwrap();

    assert_eq!(value["engine_build_id"], json!("0.0.9"));
}

#[test]
fn unwrap_core_envelope_rejects_ok_false() {
    let err = unwrap_core_envelope(json!({
        "ok": false,
        "error": { "code": "not_found", "message": "nope" }
    }))
    .unwrap_err();

    assert_eq!(err.code, "core_error");
}

#[test]
fn unwrap_core_envelope_passes_through_non_envelope() {
    let value = unwrap_core_envelope(json!({ "foo": "bar" })).unwrap();
    assert_eq!(value["foo"], json!("bar"));
}

#[test]
fn string_at_finds_first_matching_key() {
    let v = json!({ "b": "found" });
    assert_eq!(string_at(&v, &["a", "b", "c"]), Some("found".to_string()));
}

#[test]
fn u64_at_handles_string_numbers() {
    let v = json!({ "count": "42" });
    assert_eq!(u64_at(&v, &["count"]), Some(42));
}

#[test]
fn bool_at_handles_string_booleans() {
    let v = json!({ "flag": "yes" });
    assert_eq!(bool_at(&v, &["flag"]), Some(true));
}

#[test]
fn normalize_version_strips_v_prefix() {
    assert_eq!(normalize_version(Some("v0.0.5".to_string())), Some("0.0.5".to_string()));
    assert_eq!(normalize_version(Some("0.0.5".to_string())), Some("0.0.5".to_string()));
    assert_eq!(normalize_version(None), None);
}
