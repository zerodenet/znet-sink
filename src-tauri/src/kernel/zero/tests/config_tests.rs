use serde_json::json;

use crate::kernel::zero::config;

#[test]
fn proxy_nodes_extracts_outbounds() {
    let nodes = config::proxy_nodes_from_config(&json!({
        "outbounds": [
            { "tag": "direct", "type": "direct" },
            { "tag": "proxy", "type": "vless" },
            { "tag": "auto", "type": "selector" }
        ]
    }));

    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].tag, "direct");
    assert_eq!(nodes[0].protocol, "direct");
    assert!(!nodes[0].is_selector);

    assert_eq!(nodes[1].tag, "proxy");
    assert_eq!(nodes[1].protocol, "vless");

    assert_eq!(nodes[2].tag, "auto");
    assert!(nodes[2].is_selector);
}

#[test]
fn proxy_nodes_returns_empty_when_no_outbounds() {
    let nodes = config::proxy_nodes_from_config(&json!({}));
    assert!(nodes.is_empty());

    let nodes = config::proxy_nodes_from_config(&json!({ "outbounds": [] }));
    assert!(nodes.is_empty());
}

#[test]
fn policy_groups_from_config_accepts_outbound_groups() {
    let groups = config::policy_groups_from_config(&json!({
        "outbounds": [
            { "tag": "direct", "type": "direct" },
            { "tag": "server-a", "type": "trojan" },
            { "tag": "server-b", "type": "vless" }
        ],
        "outbound_groups": [
            {
                "tag": "proxy",
                "type": "selector",
                "selected": "server-b",
                "outbounds": ["server-a", "server-b", "direct"]
            }
        ]
    }));

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].tag, "proxy");
    assert_eq!(groups[0].selected.as_deref(), Some("server-b"));
    assert_eq!(
        groups[0]
            .members
            .iter()
            .map(|m| m.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["server-a", "server-b", "direct"]
    );
    assert_eq!(groups[0].members[0].kind.as_deref(), Some("trojan"));
    assert!(groups[0].members[1].selected);
}

#[test]
fn policy_groups_returns_empty_for_empty_config() {
    let groups = config::policy_groups_from_config(&json!({}));
    assert!(groups.is_empty());
}

#[test]
fn policy_groups_handles_proxy_groups_key() {
    let groups = config::policy_groups_from_config(&json!({
        "outbounds": [{ "tag": "direct", "type": "direct" }],
        "proxy_groups": [
            {
                "name": "auto",
                "type": "url_test",
                "proxies": ["direct"]
            }
        ]
    }));

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].tag, "auto");
    assert_eq!(groups[0].kind, "url_test");
}

#[test]
fn proxy_nodes_handles_protocol_fallback() {
    let nodes = config::proxy_nodes_from_config(&json!({
        "outbounds": [
            { "tag": "x", "protocol": "trojan" }
        ]
    }));

    assert_eq!(nodes[0].protocol, "trojan");
}

#[test]
fn proxy_nodes_defaults_to_unknown() {
    let nodes = config::proxy_nodes_from_config(&json!({
        "outbounds": [{ "tag": "bare" }]
    }));

    assert_eq!(nodes[0].protocol, "unknown");
}
