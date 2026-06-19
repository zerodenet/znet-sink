use gui_lib::kernel::zero::config;
use serde_json::json;

#[test]
fn proxy_nodes_extracts_outbounds() {
    let nodes = config::proxy_nodes_from_config(&json!({
        "outbounds": [
            { "tag": "direct", "type": "direct" },
            { "tag": "proxy", "type": "vless" },
            { "tag": "auto", "type": "selector" }
        ]
    }));

    // `direct` is a built-in outbound and is filtered out.
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].tag, "proxy");
    assert_eq!(nodes[0].protocol, "vless");

    assert_eq!(nodes[1].tag, "auto");
    assert!(nodes[1].is_selector);
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
    assert_eq!(groups[0].name, "proxy");
    assert_eq!(groups[0].selected.as_deref(), Some("server-b"));
    assert_eq!(
        groups[0]
            .outbounds
            .iter()
            .map(|m| m.tag.as_str())
            .collect::<Vec<_>>(),
        vec!["server-a", "server-b"] // `direct` filtered out of members
    );
    assert_eq!(groups[0].outbounds[0].kind.as_deref(), Some("trojan"));
    assert!(groups[0].outbounds[1].selected);
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
    assert_eq!(groups[0].name, "auto");
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

#[test]
fn real_world_config_extracts_nodes_and_policy_group() {
    let config = json!({
        "inbounds": [{
            "listen": { "address": "127.0.0.1", "port": 15581 },
            "protocol": { "type": "mixed" },
            "tag": "socks-in"
        }],
        "outbound_groups": [{
            "outbounds": ["ss-in", "tr-sg", "direct"],
            "tag": "proxy",
            "type": "selector"
        }],
        "outbounds": [
            { "protocol": { "type": "direct" }, "tag": "direct" },
            {
                "protocol": {
                    "cipher": "chacha20-ietf-poly1305",
                    "password": "redacted",
                    "port": 37077,
                    "server": "redacted.example.com",
                    "type": "shadowsocks",
                    "udp": true
                },
                "tag": "ss-in"
            },
            {
                "protocol": {
                    "insecure": true,
                    "password": "redacted",
                    "port": 14688,
                    "server": "redacted.example.com",
                    "sni": "redacted.example.com",
                    "type": "trojan"
                },
                "tag": "tr-sg"
            }
        ],
        "route": {
            "final": { "type": "direct" },
            "mode": { "type": "rule" },
            "rules": []
        }
    });

    let nodes = config::proxy_nodes_from_config(&config);
    // `direct` is a built-in outbound and is filtered out.
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].tag, "ss-in");
    assert_eq!(nodes[0].protocol, "shadowsocks");
    assert_eq!(nodes[1].tag, "tr-sg");
    assert_eq!(nodes[1].protocol, "trojan");

    // ── New: static attribute extraction ──
    // shadowsocks node
    assert_eq!(nodes[0].server.as_deref(), Some("redacted.example.com"));
    assert_eq!(nodes[0].port, Some(37077));
    assert_eq!(nodes[0].udp, Some(true)); // explicit `udp: true`
    assert_eq!(nodes[0].cipher.as_deref(), Some("chacha20-ietf-poly1305"));
    // trojan node — TLS inferred from sni/insecure presence
    assert_eq!(nodes[1].port, Some(14688));
    assert_eq!(nodes[1].tls, Some(true));
    assert_eq!(nodes[1].sni.as_deref(), Some("redacted.example.com"));
    // trojan has no explicit udp flag and isn't a UDP-native protocol
    assert_eq!(nodes[1].udp, None);

    let groups = config::policy_groups_from_config(&config);
    assert_eq!(groups.len(), 1, "expected one policy group");
    assert_eq!(groups[0].name, "proxy");
    assert_eq!(groups[0].kind, "selector");
    assert_eq!(groups[0].selected, None, "no selected field in this config");

    let member_tags: Vec<&str> = groups[0].outbounds.iter().map(|m| m.tag.as_str()).collect();
    assert_eq!(member_tags, vec!["ss-in", "tr-sg"]); // `direct` filtered

    let member_kinds: Vec<&str> = groups[0]
        .outbounds
        .iter()
        .filter_map(|m| m.kind.as_deref())
        .collect();
    assert_eq!(member_kinds, vec!["shadowsocks", "trojan"]);
}

#[test]
fn proxy_nodes_infers_udp_for_native_protocols() {
    let nodes = config::proxy_nodes_from_config(&json!({
        "outbounds": [
            {
                "tag": "hy",
                "protocol": { "type": "hysteria2", "server": "h.example.com", "port": 443 }
            },
            {
                "tag": "wg",
                "type": "wireguard"
            }
        ]
    }));

    // hysteria2 / wireguard are UDP-native → inferred true even without `udp` field
    assert_eq!(nodes[0].udp, Some(true));
    assert_eq!(nodes[1].udp, Some(true));
}

#[test]
fn proxy_nodes_extracts_network_and_tls() {
    let nodes = config::proxy_nodes_from_config(&json!({
        "outbounds": [
            {
                "tag": "vmess-ws",
                "protocol": {
                    "type": "vmess",
                    "server": "v.example.com",
                    "port": 443,
                    "network": "ws",
                    "tls": true,
                    "sni": "v.example.com"
                }
            }
        ]
    }));

    assert_eq!(nodes[0].network.as_deref(), Some("ws"));
    assert_eq!(nodes[0].tls, Some(true));
    assert_eq!(nodes[0].sni.as_deref(), Some("v.example.com"));
}
