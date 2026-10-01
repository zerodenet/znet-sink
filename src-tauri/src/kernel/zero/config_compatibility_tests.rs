use super::*;
use serde_json::json;

#[test]
fn wireguard_endpoint_projection_handles_domain_ipv4_and_bracketed_ipv6_without_keys() {
    for (endpoint, host) in [
        ("gateway.test:51820", "gateway.test"),
        ("192.0.2.1:51820", "192.0.2.1"),
        ("[2001:db8::1]:51820", "2001:db8::1"),
    ] {
        let source = json!({"outbounds":[{"tag":"wg","protocol":{"type":"wireguard",
            "private_key":"must-not-leak","addresses":["10.10.0.11/32"],
            "peers":[{"endpoint":endpoint,"public_key":"must-not-leak"}]}}]});
        let nodes = proxy_nodes_from_config(&source);
        assert_eq!(nodes[0].server.as_deref(), Some(host));
        assert_eq!(nodes[0].port, Some(51820));
        assert_eq!(nodes[0].udp, Some(true));
        assert!(!serde_json::to_string(&nodes)
            .unwrap()
            .contains("must-not-leak"));
    }
}

#[test]
fn multi_peer_and_invalid_endpoints_do_not_claim_a_single_device_endpoint() {
    for peers in [
        json!([]),
        json!([{"endpoint":"one:1"},{"endpoint":"two:2"}]),
        json!([{"endpoint":"missing-port"}]),
        json!([{"endpoint":":51820"}]),
        json!([{"endpoint":"host:99999"}]),
    ] {
        let nodes = proxy_nodes_from_config(
            &json!({"outbounds":[{"tag":"wg","protocol":{"type":"wireguard","peers":peers}}]}),
        );
        assert_eq!(nodes[0].server, None);
        assert_eq!(nodes[0].port, None);
    }
}

#[test]
fn v002_regular_outbound_endpoint_projection_remains_unchanged() {
    let nodes = proxy_nodes_from_config(
        &json!({"outbounds":[{"tag":"proxy","protocol":{"type":"socks5","server":"localhost","port":1080}}]}),
    );
    assert_eq!(nodes[0].protocol, "socks5");
    assert_eq!(nodes[0].server.as_deref(), Some("localhost"));
    assert_eq!(nodes[0].port, Some(1080));
}

#[test]
fn canonical_endpoints_project_only_declared_outbound_roles_even_when_disabled() {
    let config = json!({"endpoints":[
        {"tag":"wg-default","enabled":false,"protocol":{"type":"wireguard","peers":[{"endpoint":"example.test:51820"}],"private_key":"must-not-leak"}},
        {"tag":"wg-in","directions":{"inbound":true,"outbound":false},"protocol":{"type":"wireguard"}},
        {"tag":"future","directions":{"outbound":true},"protocol":{"type":"future_packet"}}
    ]});
    let nodes = proxy_nodes_from_config(&config);
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.tag.as_str())
            .collect::<Vec<_>>(),
        ["wg-default", "future"]
    );
    assert_eq!(nodes[0].server.as_deref(), Some("example.test"));
    assert_eq!(nodes[1].protocol, "future_packet");
    assert!(!serde_json::to_string(&nodes)
        .unwrap()
        .contains("must-not-leak"));
}
