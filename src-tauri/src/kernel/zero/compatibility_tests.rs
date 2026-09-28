//! Additive contracts from Zero v0.0.2 and v0.0.3 (api/query/capabilities).
use super::*;
use serde_json::json;

#[test]
fn v002_and_v003_health_keep_liveness_separate_from_peer_reachability() {
    let old = parse_health(&json!({"healthy":true,"engine_build_id":"0.0.2-rc.202609271132"}));
    assert!(old.healthy);
    assert_eq!(old.engine_version.as_deref(), Some("0.0.2-rc.202609271132"));
    assert!(old.outbound_devices.is_empty());
    let current = parse_health(&json!({
        "healthy":true,"engine_build_id":"0.0.3-dev.202609271529",
        "outbound_devices":[
            {"tag":"wg-a","peer_index":0,"state":"awaiting_handshake","endpoint_resolution_failed":false},
            {"tag":"wg-b","peer_index":1,"state":"reachable","last_handshake_age_ms":0,
             "last_authenticated_packet_age_ms":1200,"endpoint_resolution_failed":true},
            {"tag":"future","peer_index":2,"state":"future_state","future_field":true}
        ]
    }));
    assert!(current.healthy);
    assert_eq!(current.outbound_devices.len(), 3);
    assert_eq!(current.outbound_devices[0].state, "awaiting_handshake");
    assert_eq!(current.outbound_devices[0].last_handshake_age_ms, None);
    assert_eq!(current.outbound_devices[1].last_handshake_age_ms, Some(0));
    assert!(current.outbound_devices[1].endpoint_resolution_failed);
    assert_eq!(current.outbound_devices[2].state, "future_state");
    let wire = serde_json::to_value(current).unwrap();
    assert_eq!(wire["outboundDevices"][1]["peerIndex"], 1);
    assert!(wire["outboundDevices"][2].get("future_field").is_none());
}

#[test]
fn absent_and_incomplete_devices_do_not_invent_a_successful_peer() {
    for value in [
        json!({}),
        json!({"outbound_devices":null}),
        json!({"outbound_devices":[{}, {"tag":"wg"}, {"peer_index":0}]}),
    ] {
        assert!(parse_health(&value).outbound_devices.is_empty());
    }
}

#[test]
fn protocol_metadata_preserves_opt_in_and_old_missing_compilation_status() {
    for compiled in [true, false] {
        let caps = parse_capabilities(
            &json!({"protocols":[{
                "protocol":"wireguard", "feature":"wireguard", "compiled":compiled,
                "status":"experimental", "compatibility_baseline":"gotatun_v0.9.2", "transports":["udp"],
                "outbound":{"tcp":{"supported":compiled,"level":"experimental","notes":["shared_peer_tcp_tunnel"]},
                            "udp":{"supported":compiled,"level":"experimental"}},
                "mux":{"supported":false,"level":"not_applicable"}, "limitations":["no_kernel_tun"]
            }]}),
            None,
        );
        let protocol = &caps.protocols[0];
        assert_eq!(protocol.compiled, Some(compiled));
        assert_eq!(protocol.outbound_tcp, compiled);
        assert_eq!(protocol.status, "experimental");
        assert_eq!(
            protocol.compatibility_baseline.as_deref(),
            Some("gotatun_v0.9.2")
        );
        assert_eq!(
            protocol.outbound_tcp_state.notes,
            ["shared_peer_tcp_tunnel"]
        );
        assert_eq!(protocol.transports, ["udp"]);
        assert!(!protocol.mux);
    }
    let old = parse_capabilities(
        &json!({"protocols":[{"name":"socks5","outbound_tcp":true}]}),
        None,
    );
    assert_eq!(old.protocols[0].compiled, None);
    assert!(old.protocols[0].outbound_tcp);
}

/// Run after scripts/check-kernel-compatibility.py has captured both disposable
/// runtimes. Uses the actual wire data, rather than inferred product versions.
#[test]
#[ignore = "requires captures from two real disposable kernels"]
fn real_kernel_version_matrix() {
    let directory = std::env::var("ZNET_KERNEL_COMPATIBILITY_CAPTURES").unwrap();
    for series in ["0.0.2", "0.0.3"] {
        let value: Value = serde_json::from_str(
            &std::fs::read_to_string(
                std::path::Path::new(&directory).join(format!("{series}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let health = parse_health(&value["health"]);
        assert!(health.healthy);
        assert_eq!(health.engine_version.as_deref(), value["version"].as_str());
        let capabilities = parse_capabilities(&value["capabilities"], None);
        assert!(capabilities.available);
        assert_eq!(capabilities.api_version.as_deref(), Some("zero.api.v1"));
        assert_eq!(
            capabilities.contracts.as_ref().unwrap().control_api.current,
            1
        );
        for protocol in &capabilities.protocols {
            let raw = value["capabilities"]["protocols"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["protocol"] == protocol.name)
                .unwrap();
            assert_eq!(protocol.compiled, raw["compiled"].as_bool());
            assert_eq!(protocol.outbound_tcp, raw["outbound"]["tcp"]["supported"]);
            assert_eq!(protocol.status, raw["status"]);
        }
        assert_eq!(parse_stats(&value["stats"]).active_sessions, 0);
        assert!(parse_policy_groups(&value["policies"]).is_empty());
        assert!(
            !super::super::runtime::parse_tun_status(&value["tun_status"])
                .unwrap()
                .enabled
        );
    }
}
