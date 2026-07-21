use gui_lib::kernel::zero::events;
use gui_lib::kernel::zero::parsing;
use gui_lib::models::gui_core::GuiEventData;
use serde_json::json;

#[test]
fn policy_probe_completed_preserves_runtime_details() {
    let event = events::normalize_event(&json!({
        "schema_id": "zero.event.v1",
        "event_type": "policy.probe.completed",
        "payload": {
            "policy_tag": "auto",
            "trigger": "manual",
            "url": "http://www.gstatic.com/generate_204",
            "started_at_unix_ms": 1710000000000_u64,
            "completed_at_unix_ms": 1710000000320_u64,
            "duration_ms": 320,
            "selected": "server-b",
            "members": [{
                "target_tag": "server-a",
                "healthy": false,
                "latency_ms": 120,
                "error": "timeout"
            }]
        }
    }));

    let GuiEventData::PolicyProbeCompleted(probe) = event.payload else {
        panic!("expected policy probe event");
    };
    assert_eq!(probe.policy_tag, "auto");
    assert_eq!(probe.trigger.as_deref(), Some("manual"));
    assert_eq!(probe.duration_ms, Some(320));
    assert_eq!(probe.selected.as_deref(), Some("server-b"));
    assert_eq!(probe.members[0].alive, Some(false));
    assert_eq!(probe.members[0].delay_ms, Some(120));
    assert_eq!(probe.members[0].last_error.as_deref(), Some("timeout"));
}

#[test]
fn policy_probe_completed_maps_selected_member_snapshot() {
    let event = events::normalize_event(&json!({
        "schema_id": "zero.event.v1",
        "event_type": "policy.probe.completed",
        "occurred_at_unix_ms": 1784294531467_u64,
        "payload": {
            "completed_at_unix_ms": 1784294531466_u64,
            "duration_ms": 1610,
            "members": [
                {
                    "error": null,
                    "healthy": true,
                    "latency_ms": 794,
                    "target_tag": "ss-in"
                },
                {
                    "error": null,
                    "healthy": true,
                    "latency_ms": 815,
                    "target_tag": "tr-sg"
                }
            ],
            "policy_tag": "Auto-proxy",
            "selected": "ss-in",
            "started_at_unix_ms": 1784294529856_u64,
            "trigger": "scheduled",
            "url": "http://latency-test.skk.moe/endpoint"
        }
    }));

    let GuiEventData::PolicyProbeCompleted(probe) = event.payload else {
        panic!("expected policy probe event");
    };
    assert_eq!(probe.policy_tag, "Auto-proxy");
    assert_eq!(probe.trigger.as_deref(), Some("scheduled"));
    assert_eq!(probe.completed_at_unix_ms, Some(1784294531466));
    assert_eq!(probe.selected.as_deref(), Some("ss-in"));
    assert_eq!(probe.members.len(), 2);
    assert_eq!(probe.members[0].tag, "ss-in");
    assert!(probe.members[0].selected);
    assert_eq!(probe.members[0].delay_ms, Some(794));
    assert_eq!(probe.members[1].tag, "tr-sg");
    assert!(!probe.members[1].selected);
    assert_eq!(probe.members[1].delay_ms, Some(815));
}

// ── stats ──

#[test]
fn parse_stats_accepts_current_zero_fields() {
    let stats = parsing::parse_stats(&json!({
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

// ── health ──

#[test]
fn parse_health_maps_engine_build_id() {
    let health = parsing::parse_health(&json!({
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
    let health = parsing::parse_health(&json!({
        "engine_build_id": "v0.0.5",
        "healthy": true
    }));

    assert_eq!(health.engine_version.as_deref(), Some("0.0.5"));
}

#[test]
fn parse_health_defaults_healthy_when_missing() {
    let health = parsing::parse_health(&json!({}));

    assert!(health.healthy);
    assert!(health.engine_version.is_none());
    assert!(health.started_at_unix_ms.is_none());
}

// ── capabilities ──

#[test]
fn parse_capabilities_maps_api_id_field() {
    let caps = parsing::parse_capabilities(
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
fn parse_capabilities_maps_protocol_matrix_and_build_features() {
    let caps = parsing::parse_capabilities(
        &json!({
            "api_id": "zero.api.v1",
            "build_features": ["tun", "quic"],
            "protocols": [
                {
                    "name": "shadowsocks",
                    "status": "supported",
                    "inbound_tcp": true,
                    "inbound_udp": true,
                    "outbound_tcp": true,
                    "outbound_udp": true,
                    "mux": true,
                    "limitations": []
                },
                {
                    "protocol": "vmess",
                    "status": "partial",
                    "inboundTcp": true,
                    "inboundUdp": false,
                    "outboundTcp": true,
                    "outboundUdp": false,
                    "limitations": ["no_udp_relay"]
                }
            ]
        }),
        None,
    );

    assert_eq!(caps.build_features, vec!["tun", "quic"]);
    assert_eq!(caps.protocols.len(), 2);
    assert_eq!(caps.protocols[0].name, "shadowsocks");
    assert_eq!(caps.protocols[0].status, "supported");
    assert!(caps.protocols[0].inbound_tcp);
    assert!(caps.protocols[0].inbound_udp);
    assert!(caps.protocols[0].outbound_tcp);
    assert!(caps.protocols[0].outbound_udp);
    assert!(caps.protocols[0].mux);
    assert_eq!(caps.protocols[1].name, "vmess");
    assert_eq!(caps.protocols[1].status, "partial");
    assert!(caps.protocols[1].inbound_tcp);
    assert!(!caps.protocols[1].inbound_udp);
    assert!(caps.protocols[1].outbound_tcp);
    assert!(!caps.protocols[1].outbound_udp);
    assert_eq!(caps.protocols[1].limitations, vec!["no_udp_relay"]);
}

#[test]
fn parse_capabilities_captures_error() {
    let caps = parsing::parse_capabilities(&json!({}), Some("connection refused".to_string()));

    assert!(!caps.available);
    assert_eq!(caps.error.as_deref(), Some("connection refused"));
}

// ── connections ──

#[test]
fn parse_connection_accepts_nested_target() {
    let conn = parsing::parse_connection(&json!({
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
    let conn = parsing::parse_connection(&json!({
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
    assert!(parsing::parse_connection(&json!({ "host": "x" })).is_none());
}

// ── envelope ──

#[test]
fn unwrap_core_envelope_strips_ok_result() {
    let value = parsing::unwrap_core_envelope(json!({
        "ok": true,
        "result": { "engine_build_id": "0.0.9" }
    }))
    .unwrap();

    assert_eq!(value["engine_build_id"], json!("0.0.9"));
}

#[test]
fn unwrap_core_envelope_rejects_ok_false() {
    let err = parsing::unwrap_core_envelope(json!({
        "ok": false,
        "error": { "code": "not_found", "message": "nope" }
    }))
    .unwrap_err();

    assert_eq!(err.code, "core_error");
}

#[test]
fn unwrap_core_envelope_passes_through_non_envelope() {
    let value = parsing::unwrap_core_envelope(json!({ "foo": "bar" })).unwrap();
    assert_eq!(value["foo"], json!("bar"));
}

// ── QueryResponse variant unwrapping ──

#[test]
fn unwrap_query_variant_returns_config_entity() {
    let result = parsing::unwrap_query_variant(
        json!({
            "api_id": "zero.api.v1",
            "ok": true,
            "id": "config-1",
            "result": {
                "config": {
                    "inbounds": [],
                    "outbounds": [{ "tag": "direct", "protocol": { "type": "direct" } }]
                }
            }
        }),
        "config",
    )
    .unwrap();

    assert!(result.get("api_id").is_none());
    assert!(result.get("result").is_none());
    assert_eq!(result["outbounds"][0]["tag"], json!("direct"));
}

#[test]
fn parse_connection_preserves_canonical_lifecycle_record() {
    let conn = parsing::parse_connection(&json!({
        "flow_id": "legacy-id",
        "record": {
            "flow_id": "50dbde45",
            "revision": 7,
            "state": "completed",
            "network": "tcp",
            "inbound": { "tag": "DEFAULT-TPROXY", "protocol": "tproxy" },
            "source": {
                "ip": "192.168.50.175",
                "port": 52864,
                "process_id": 98,
                "process_name": "Courier"
            },
            "target": {
                "host": "19-courier.push.apple.com",
                "port": 5223,
                "resolved_ip": "17.57.147.6",
                "sniffed_host": "courier.push.apple.com"
            },
            "route": {
                "mode": "rule",
                "action": "route",
                "target": "Proxy",
                "matched_rule": { "index": 12, "condition": "domain_suffix:apple.com" },
                "selection_chain": ["Proxy", "US-Lite"]
            },
            "path": {
                "outbound": { "tag": "US-Lite", "protocol": "vmess" },
                "remote": { "host": "8.163.113.57", "port": 443 },
                "relay_chain": [
                    { "tag": "entry", "protocol": "shadowsocks" },
                    { "tag": "US-Lite", "protocol": "vmess" }
                ]
            },
            "traffic": {
                "bytes_up": 12294,
                "bytes_down": 4790,
                "inbound_rx_bytes": 6147,
                "inbound_tx_bytes": 2395,
                "outbound_rx_bytes": 2395,
                "outbound_tx_bytes": 6147
            },
            "throughput": {
                "upload_bps": 0,
                "download_bps": 63,
                "sampled_at_unix_ms": 1784481785000_u64
            },
            "timing": {
                "started_at_unix_ms": 1784481784193_u64,
                "last_activity_at_unix_ms": 1784481784900_u64,
                "ended_at_unix_ms": 1784481785000_u64,
                "duration_ms": 807
            },
            "result": {
                "outcome": "failed",
                "close_reason": "upstream_error",
                "failure": {
                    "stage": "relay",
                    "code": "io",
                    "message": "connection reset"
                }
            }
        }
    }))
    .expect("parse canonical record");

    assert_eq!(conn.flow_id, "50dbde45");
    assert_eq!(conn.revision, Some(7));
    assert_eq!(conn.source.as_deref(), Some("192.168.50.175:52864"));
    assert_eq!(conn.process_name.as_deref(), Some("Courier"));
    assert_eq!(conn.destination, "19-courier.push.apple.com:5223");
    assert_eq!(conn.target_ip.as_deref(), Some("17.57.147.6"));
    assert_eq!(conn.matched_rule_index, Some(12));
    assert_eq!(conn.selection_chain, vec!["Proxy", "US-Lite"]);
    assert_eq!(conn.relay_chain, vec!["entry", "US-Lite"]);
    assert_eq!(conn.remote_destination.as_deref(), Some("8.163.113.57:443"));
    assert_eq!(conn.failure_stage.as_deref(), Some("relay"));
    assert_eq!(conn.failure_code.as_deref(), Some("io"));
    assert_eq!(conn.bytes_up, 12294);
    assert_eq!(conn.outbound_tx_bytes, Some(6147));
    assert_eq!(conn.duration_ms, Some(807));
}

#[test]
fn boxed_connection_event_preserves_the_gui_json_contract() {
    let event = events::normalize_event(&json!({
        "event_type": "flow.started",
        "payload": {
            "flow_id": "flow-1",
            "network": "tcp",
            "target": { "host": "example.com", "port": 443 },
            "traffic": { "bytes_up": 10, "bytes_down": 20 }
        }
    }));

    let wire = serde_json::to_value(&event).expect("serialize GUI event");
    assert_eq!(wire["payload"]["kind"], json!("connection"));
    assert_eq!(wire["payload"]["data"]["flowId"], json!("flow-1"));
    assert_eq!(
        wire["payload"]["data"]["destination"],
        json!("example.com:443")
    );

    let GuiEventData::Connection(connection) = event.payload else {
        panic!("expected connection event");
    };
    assert_eq!(connection.flow_id, "flow-1");
}

#[test]
fn flow_snapshot_normalizes_to_connection_snapshot() {
    let event = events::normalize_event(&json!({
        "event_type": "flow.snapshot",
        "sequence": 9,
        "payload": {
            "watermark": 9,
            "records": [{
                "flow_id": "1",
                "revision": 3,
                "state": "active",
                "network": "udp",
                "inbound": { "tag": "socks-in", "protocol": "socks5" },
                "target": { "host": "dns.example", "port": 53 },
                "route": { "mode": "rule", "action": "direct", "selection_chain": [] },
                "path": { "relay_chain": [] },
                "traffic": { "bytes_up": 10, "bytes_down": 20 },
                "throughput": { "upload_bps": 1, "download_bps": 2, "sampled_at_unix_ms": 100 },
                "timing": { "started_at_unix_ms": 50, "last_activity_at_unix_ms": 90 }
            }]
        }
    }));

    assert_eq!(event.event_type, "connection.snapshot");
    let GuiEventData::Connections(connections) = event.payload else {
        panic!("expected connection snapshot");
    };
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].flow_id, "1");
    assert_eq!(connections[0].revision, Some(3));
    assert_eq!(connections[0].bytes_down, 20);
}

#[test]
fn unwrap_query_variant_returns_policies_entity() {
    let result = parsing::unwrap_query_variant(
        json!({
            "api_id": "zero.api.v1",
            "ok": true,
            "id": "policies-1",
            "result": {
                "policies": [{ "policy_tag": "auto", "selected": "server-b" }]
            }
        }),
        "policies",
    )
    .unwrap();

    assert!(result.is_array());
    assert_eq!(result[0]["policy_tag"], json!("auto"));
    assert_eq!(result[0]["selected"], json!("server-b"));
}

#[test]
fn unwrap_query_variant_strips_health_key() {
    let result = parsing::unwrap_query_variant(
        json!({
            "ok": true,
            "result": {
                "health": {
                    "engine_build_id": "0.0.9",
                    "started_at_unix_ms": 1713500000000_u64,
                    "healthy": true
                }
            }
        }),
        "health",
    )
    .unwrap();

    assert_eq!(result["engine_build_id"], json!("0.0.9"));
    assert_eq!(result["healthy"], json!(true));
}

#[test]
fn unwrap_query_variant_strips_capabilities_key() {
    let result = parsing::unwrap_query_variant(
        json!({
            "ok": true,
            "result": {
                "capabilities": {
                    "api_id": "zero.api.v1",
                    "features": ["query"],
                    "permissions": [],
                    "adapters": [],
                    "sinks": []
                }
            }
        }),
        "capabilities",
    )
    .unwrap();

    assert_eq!(result["api_id"], json!("zero.api.v1"));
    assert_eq!(result["features"], json!(["query"]));
}

#[test]
fn unwrap_query_variant_strips_active_flows_key() {
    let result = parsing::unwrap_query_variant(
        json!({
            "ok": true,
            "result": {
                "active_flows": [
                    { "flow_id": "1", "network": "tcp" },
                    { "flow_id": "2", "network": "udp" }
                ]
            }
        }),
        "active_flows",
    )
    .unwrap();

    assert!(result.is_array());
    assert_eq!(result.as_array().unwrap().len(), 2);
}

#[test]
fn unwrap_query_variant_strips_tun_status_key() {
    let result = parsing::unwrap_query_variant(
        json!({
            "ok": true,
            "result": {
                "tun_status": {
                    "running": true,
                    "name": "utun0",
                    "addr": "10.0.0.1/24",
                    "tag": "tun-in"
                }
            }
        }),
        "tun_status",
    )
    .unwrap();

    assert_eq!(result["running"], json!(true));
    assert_eq!(result["name"], json!("utun0"));
}

#[test]
fn unwrap_query_variant_falls_back_to_flat_shape() {
    let result = parsing::unwrap_query_variant(
        json!({
            "ok": true,
            "result": {
                "engine_build_id": "0.0.8",
                "healthy": true
            }
        }),
        "health",
    )
    .unwrap();

    assert_eq!(result["engine_build_id"], json!("0.0.8"));
}

#[test]
fn unwrap_query_variant_rejects_ok_false() {
    let err = parsing::unwrap_query_variant(
        json!({
            "ok": false,
            "error": { "code": "not_found", "message": "nope" }
        }),
        "health",
    )
    .unwrap_err();

    assert_eq!(err.code, "core_error");
}

#[test]
fn full_health_roundtrip_with_variant() {
    let response = json!({
        "api_id": "zero.api.v1",
        "ok": true,
        "id": 1,
        "result": {
            "health": {
                "engine_build_id": "0.0.10",
                "started_at_unix_ms": 1713500000000_u64,
                "healthy": true
            }
        }
    });

    let inner = parsing::unwrap_query_variant(response, "health").unwrap();
    let health = parsing::parse_health(&inner);

    assert!(health.healthy);
    assert_eq!(health.engine_version.as_deref(), Some("0.0.10"));
    assert_eq!(health.started_at_unix_ms, Some(1713500000000));
}

#[test]
fn full_capabilities_roundtrip_with_variant() {
    let response = json!({
        "api_id": "zero.api.v1",
        "ok": true,
        "result": {
            "capabilities": {
                "api_id": "zero.api.v1",
                "schema_id": "zero.event.v1",
                "features": ["query", "config_snapshot", "runtime_snapshot"],
                "permissions": ["read"],
                "adapters": [{ "kind": "in_process", "enabled": true }],
                "sinks": []
            }
        }
    });

    let inner = parsing::unwrap_query_variant(response, "capabilities").unwrap();
    let caps = parsing::parse_capabilities(&inner, None);

    assert!(caps.available);
    assert_eq!(
        caps.features,
        vec!["query", "config_snapshot", "runtime_snapshot"]
    );
}

#[test]
fn full_active_flows_roundtrip_with_variant() {
    let response = json!({
        "api_id": "zero.api.v1",
        "ok": true,
        "result": {
            "active_flows": [
                {
                    "flow_id": "abc-123",
                    "network": "tcp",
                    "target": { "host": "example.com", "port": 443 },
                    "traffic": { "bytes_up": 500, "bytes_down": 1500 }
                }
            ]
        }
    });

    let inner = parsing::unwrap_query_variant(response, "active_flows").unwrap();
    let list = parsing::parse_connection_list(&inner, 100);

    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].flow_id, "abc-123");
    assert_eq!(list.items[0].destination, "example.com:443");
}

// ── utility functions ──

#[test]
fn string_at_finds_first_matching_key() {
    let v = json!({ "b": "found" });
    assert_eq!(
        parsing::string_at(&v, &["a", "b", "c"]),
        Some("found".to_string())
    );
}

#[test]
fn u64_at_handles_string_numbers() {
    let v = json!({ "count": "42" });
    assert_eq!(parsing::u64_at(&v, &["count"]), Some(42));
}

#[test]
fn bool_at_handles_string_booleans() {
    let v = json!({ "flag": "yes" });
    assert_eq!(parsing::bool_at(&v, &["flag"]), Some(true));
}

#[test]
fn normalize_version_strips_v_prefix() {
    assert_eq!(
        parsing::normalize_version(Some("v0.0.5".to_string())),
        Some("0.0.5".to_string())
    );
    assert_eq!(
        parsing::normalize_version(Some("0.0.5".to_string())),
        Some("0.0.5".to_string())
    );
    assert_eq!(parsing::normalize_version(None), None);
}

// ── plan_apply result parsing ──

#[test]
fn plan_apply_parses_full_impact_analysis() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true,
        "hot_reload": [
            { "section": "outbounds", "tags": ["proxy-us", "proxy-jp"], "detail": "2 outbound proxies updated" },
            { "section": "rules", "tags": [], "detail": "5 routing rules changed" }
        ],
        "requires_restart": [
            { "section": "listeners", "tags": ["mixed-in"], "detail": "listening port changed from 1080 to 1081" }
        ],
        "warnings": ["Active connections may be interrupted"],
        "errors": []
    }));

    assert!(result.valid);
    assert_eq!(result.hot_reload.len(), 2);
    assert_eq!(result.hot_reload[0].section, "outbounds");
    assert_eq!(result.hot_reload[0].tags, vec!["proxy-us", "proxy-jp"]);
    assert_eq!(result.hot_reload[0].detail, "2 outbound proxies updated");
    assert_eq!(result.hot_reload[1].section, "rules");
    assert!(result.hot_reload[1].tags.is_empty());
    assert_eq!(result.requires_restart.len(), 1);
    assert_eq!(result.requires_restart[0].section, "listeners");
    assert_eq!(result.requires_restart[0].tags, vec!["mixed-in"]);
    assert_eq!(
        result.warnings,
        vec!["Active connections may be interrupted"]
    );
    assert!(result.errors.is_empty());
}

#[test]
fn plan_apply_parses_hot_reload_only() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true,
        "hot_reload": [
            { "section": "outbounds", "tags": ["proxy-sg"], "detail": "1 outbound updated" }
        ],
        "requires_restart": [],
        "warnings": [],
        "errors": []
    }));

    assert!(result.valid);
    assert_eq!(result.hot_reload.len(), 1);
    assert!(result.requires_restart.is_empty());
    assert!(result.warnings.is_empty());
}

#[test]
fn plan_apply_parses_restart_only() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true,
        "hot_reload": [],
        "requires_restart": [
            { "section": "tun", "tags": [], "detail": "TUN address changed" },
            { "section": "dns", "tags": [], "detail": "DNS server changed" }
        ],
        "warnings": ["Kernel must be restarted for TUN changes"],
        "errors": []
    }));

    assert!(result.valid);
    assert!(result.hot_reload.is_empty());
    assert_eq!(result.requires_restart.len(), 2);
    assert_eq!(result.requires_restart[0].section, "tun");
    assert_eq!(result.requires_restart[1].section, "dns");
    assert_eq!(result.warnings.len(), 1);
}

#[test]
fn plan_apply_parses_invalid_config() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": false,
        "hot_reload": [],
        "requires_restart": [],
        "warnings": [],
        "errors": ["unknown field `foobar` at line 42", "missing required field `outbounds`"]
    }));

    assert!(!result.valid);
    assert!(result.hot_reload.is_empty());
    assert!(result.requires_restart.is_empty());
    assert_eq!(result.errors.len(), 2);
    assert_eq!(result.errors[0], "unknown field `foobar` at line 42");
}

#[test]
fn plan_apply_unwraps_result_envelope() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true,
        "result": {
            "valid": true,
            "hot_reload": [
                { "section": "config", "tags": [], "detail": "all changes" }
            ],
            "requires_restart": [],
            "warnings": [],
            "errors": []
        }
    }));

    assert!(result.valid);
    assert_eq!(result.hot_reload.len(), 1);
    assert_eq!(result.hot_reload[0].section, "config");
    assert!(result.requires_restart.is_empty());
}

#[test]
fn plan_apply_defaults_to_empty_on_minimal_input() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true
    }));

    assert!(result.valid);
    assert!(result.hot_reload.is_empty());
    assert!(result.requires_restart.is_empty());
    assert!(result.warnings.is_empty());
    assert!(result.errors.is_empty());
}

#[test]
fn plan_apply_defaults_valid_to_true_when_missing() {
    let result = parsing::parse_plan_apply_result(&json!({}));

    assert!(result.valid);
    assert!(result.hot_reload.is_empty());
    assert!(result.requires_restart.is_empty());
}

#[test]
fn plan_apply_tolerates_missing_detail_in_items() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true,
        "hot_reload": [
            { "section": "outbounds", "tags": ["proxy-us"] }
        ],
        "requires_restart": [
            { "section": "listeners" }
        ],
        "warnings": [],
        "errors": []
    }));

    assert_eq!(result.hot_reload[0].detail, "");
    assert!(!result.hot_reload[0].tags.is_empty());
    assert_eq!(result.requires_restart[0].detail, "");
    assert!(result.requires_restart[0].tags.is_empty());
}

#[test]
fn plan_accepts_alternative_item_key_names() {
    let result = parsing::parse_plan_apply_result(&json!({
        "valid": true,
        "hot_reload": [
            { "name": "routing", "affected": ["rule-a", "rule-b"], "description": "2 rules changed" }
        ],
        "requires_restart": [
            { "key": "dns", "message": "DNS upstream changed" }
        ],
        "warnings": ["Some warning"],
        "errors": []
    }));

    assert_eq!(result.hot_reload[0].section, "routing");
    assert_eq!(result.hot_reload[0].tags, vec!["rule-a", "rule-b"]);
    assert_eq!(result.hot_reload[0].detail, "2 rules changed");
    assert_eq!(result.requires_restart[0].section, "dns");
    assert_eq!(result.requires_restart[0].detail, "DNS upstream changed");
}

// ── probe target parsing ──

#[test]
fn target_probe_accepts_diagnostics_probe_target_response() {
    let result = parsing::parse_target_probe(
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
    let result = parsing::parse_target_probe(
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
    let result =
        parsing::parse_target_probe(&json!({ "reachable": true }), "fallback-tag".to_string());

    assert_eq!(result.target_tag, "fallback-tag");
}

#[test]
fn target_probe_accepts_latency_ms_field() {
    let result = parsing::parse_target_probe(
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
    let result = parsing::parse_target_probe(
        &json!({
            "target_tag": "x",
            "reachable": true,
            "delay_ms": 75
        }),
        "x".to_string(),
    );

    assert_eq!(result.latency_ms, Some(75));
}
