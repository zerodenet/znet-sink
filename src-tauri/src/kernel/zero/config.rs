//! Static config file parsing for the Zero kernel.
//!
//! Extracts proxy nodes and policy groups directly from the config JSON
//! on disk. Works even when the kernel is not running — used for the
//! node list and selector dropdown. Runtime status (selected, latency,
//! alive) is layered on top from the kernel's Policies query when connected.

use serde_json::Value;

use crate::models::gui_core::{
    ConfigProxyNode, GuiPolicyGroup, GuiPolicyMember,
};

use super::parsing::{string_at, values_from_container};

/// Protocols that are UDP-capable by design — when the config omits an
/// explicit `udp` flag for these, we assume `true` so the node card can
/// show an accurate UDP badge.
const UDP_BY_DEFAULT: &[&str] = &[
    "hysteria",
    "hysteria2",
    "tuic",
    "wireguard",
    "shadowsocks",
    "socks",
];

/// Extract proxy nodes from the active proxy config file content.
pub fn proxy_nodes_from_config(config_content: &Value) -> Vec<ConfigProxyNode> {
    let outbounds = config_content
        .get("outbounds")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    outbounds
        .iter()
        .filter_map(|node| {
            let tag = node.get("tag").and_then(|v| v.as_str())?;
            // Zero outbound format: {"tag":"...", "protocol":{"type":"shadowsocks", ...}}
            // Also support flat format: {"tag":"...", "type":"shadowsocks"}
            let protocol = resolve_outbound_protocol(node).to_string();
            // Skip built-in outbounds (direct/block/reject/dns) — they are
            // internal routing chains injected by the kernel/subscription,
            // not real proxy nodes, and must never appear in the node list
            // or as group members.
            if is_builtin_outbound(&protocol) {
                return None;
            }
            let protocol_obj = node.get("protocol").and_then(|p| p.as_object());
            Some(ConfigProxyNode {
                tag: tag.to_string(),
                protocol: protocol.clone(),
                is_selector: protocol.eq_ignore_ascii_case("selector"),
                server: protocol_obj
                    .and_then(|o| string_at_obj(o, &["server", "address", "host"]))
                    .or_else(|| string_at(node, &["server", "address", "host"])),
                port: protocol_obj
                    .and_then(|o| u16_at_obj(o, &["port"]))
                    .or_else(|| u16_at_value(node, &["port"])),
                udp: resolve_udp(&protocol, protocol_obj, node),
                network: protocol_obj
                    .and_then(|o| string_at_obj(o, &["network", "transport"]))
                    .map(|n| n.to_lowercase()),
                tls: resolve_tls(protocol_obj, node),
                sni: protocol_obj
                    .and_then(|o| string_at_obj(o, &["sni", "server_name", "serverName"]))
                    .or_else(|| string_at(node, &["sni"])),
                cipher: protocol_obj
                    .and_then(|o| string_at_obj(o, &["cipher", "security", "method"]))
                    .or_else(|| string_at(node, &["cipher"])),
            })
        })
        .collect()
}

/// Whether a protocol/tag names a built-in outbound — an internal routing
/// chain like `direct`/`block`/`reject` rather than a real proxy node. Used
/// to filter built-ins out of both the node list and group members so they
/// never show up in the UI.
fn is_builtin_outbound(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "direct" | "block" | "reject" | "dns" | "pass"
    )
}

/// Resolve the protocol name from an outbound definition.
///
/// Zero uses `{"protocol": {"type": "shadowsocks", ...}}` (nested object).
/// Also handles the flat format `{"type": "shadowsocks"}` and the
/// string shorthand `{"protocol": "shadowsocks"}`.
fn resolve_outbound_protocol(node: &Value) -> &str {
    // Nested: node.protocol.type
    node.get("protocol")
        .and_then(|p| p.get("type").and_then(|v| v.as_str()))
        // Flat: node.type
        .or_else(|| node.get("type").and_then(|v| v.as_str()))
        // String: node.protocol (as bare string)
        .or_else(|| node.get("protocol").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
}

/// Resolve UDP support.  Honors an explicit `udp` field when present;
/// otherwise infers from the protocol name for UDP-native transports.
fn resolve_udp(
    protocol: &str,
    protocol_obj: Option<&serde_json::Map<String, Value>>,
    node: &Value,
) -> Option<bool> {
    if let Some(udp) = protocol_obj.and_then(|o| bool_at_obj(o, &["udp"])) {
        return Some(udp);
    }
    if let Some(udp) = bool_at_value(node, &["udp"]) {
        return Some(udp);
    }
    let proto_lower = protocol.to_ascii_lowercase();
    if UDP_BY_DEFAULT.iter().any(|p| proto_lower.contains(p)) {
        return Some(true);
    }
    None
}

/// Resolve TLS usage.  True when `tls` is explicitly enabled, or when
/// TLS-implying fields (`sni`, `alpn`, `insecure`) are present.
fn resolve_tls(
    protocol_obj: Option<&serde_json::Map<String, Value>>,
    node: &Value,
) -> Option<bool> {
    if let Some(tls) = protocol_obj.and_then(|o| bool_at_obj(o, &["tls", "tls_enabled"])) {
        return Some(tls);
    }
    if let Some(tls) = bool_at_value(node, &["tls"]) {
        return Some(tls);
    }
    let has_tls_marker = protocol_obj
        .map(|o| {
            o.contains_key("sni")
                || o.contains_key("alpn")
                || o.contains_key("insecure")
                || o.contains_key("cert")
        })
        .unwrap_or(false);
    if has_tls_marker {
        Some(true)
    } else {
        None
    }
}

// ── Local scalar extractors over `serde_json::Map` ──────────────────
// The shared `parsing::string_at` operates on `&Value`; these thin helpers
// read directly from a `Map` so we can dig into the nested `protocol` object.

fn string_at_obj(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| obj.get(*k).and_then(Value::as_str).map(String::from))
}

fn u16_at_obj(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u16> {
    keys.iter().find_map(|k| {
        obj.get(*k).and_then(|v| {
            v.as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .or_else(|| v.as_str().and_then(|s| s.parse::<u16>().ok()))
        })
    })
}

fn u16_at_value(value: &Value, keys: &[&str]) -> Option<u16> {
    keys.iter().find_map(|k| {
        value.get(*k).and_then(|v| {
            v.as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .or_else(|| v.as_str().and_then(|s| s.parse::<u16>().ok()))
        })
    })
}

fn bool_at_obj(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|k| {
        obj.get(*k).and_then(|v| {
            v.as_bool().or_else(|| {
                v.as_str().and_then(|s| match s.to_ascii_lowercase().as_str() {
                    "true" | "yes" | "1" => Some(true),
                    "false" | "no" | "0" => Some(false),
                    _ => None,
                })
            })
        })
    })
}

fn bool_at_value(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|k| {
        value.get(*k).and_then(|v| {
            v.as_bool().or_else(|| {
                v.as_str().and_then(|s| match s.to_ascii_lowercase().as_str() {
                    "true" | "yes" | "1" => Some(true),
                    "false" | "no" | "0" => Some(false),
                    _ => None,
                })
            })
        })
    })
}

/// Extract policy groups from the active proxy config file content.
pub fn policy_groups_from_config(config_content: &Value) -> Vec<GuiPolicyGroup> {
    let outbound_kinds = outbound_kind_map(config_content);

    values_from_container(
        config_content,
        &[
            "outbound_groups",
            "outboundGroups",
            "policy_groups",
            "policyGroups",
            "policies",
            "proxy-groups",
            "proxy_groups",
            "groups",
        ],
    )
    .into_iter()
    .filter_map(|group| parse_config_policy_group(group, &outbound_kinds))
    .collect()
}

fn parse_config_policy_group(
    value: Value,
    outbound_kinds: &std::collections::HashMap<String, String>,
) -> Option<GuiPolicyGroup> {
    let tag = string_at(&value, &["tag", "name", "id", "policy_tag", "policyTag"])?;
    let selected = string_at(&value, &["selected", "current", "now", "target", "default"]);
    let members = parse_config_policy_members(&value, selected.as_deref(), outbound_kinds);

    Some(GuiPolicyGroup {
        name: tag,
        kind: string_at(&value, &["type", "kind", "policy_kind", "policyKind"])
            .unwrap_or_else(|| "selector".to_string()),
        selected,
        outbounds: members,
        available: true,
        reason: None,
    })
}

fn parse_config_policy_members(
    value: &Value,
    selected: Option<&str>,
    outbound_kinds: &std::collections::HashMap<String, String>,
) -> Vec<GuiPolicyMember> {
    values_from_container(
        value,
        &[
            "members",
            "targets",
            "children",
            "outbounds",
            "proxies",
            "items",
        ],
    )
    .into_iter()
    .filter_map(|member| parse_config_policy_member(member, selected, outbound_kinds))
    .collect()
}

fn parse_config_policy_member(
    value: Value,
    selected: Option<&str>,
    outbound_kinds: &std::collections::HashMap<String, String>,
) -> Option<GuiPolicyMember> {
    let tag = match &value {
        Value::String(tag) => tag.clone(),
        other => string_at(
            other,
            &["tag", "target_tag", "targetTag", "name", "id", "target"],
        )?,
    };
    // Skip built-in outbounds referenced as group members — they are not
    // selectable proxy nodes.
    if is_builtin_outbound(&tag) {
        return None;
    }
    let kind = value
        .as_object()
        .and_then(|_| string_at(&value, &["kind", "type", "protocol"]))
        .or_else(|| outbound_kinds.get(&tag).cloned());

    Some(GuiPolicyMember {
        selected: selected.is_some_and(|selected| selected == tag),
        tag,
        kind,
        alive: None,
        delay_ms: None,
    })
}

pub fn outbound_kind_map(value: &Value) -> std::collections::HashMap<String, String> {
    values_from_container(value, &["outbounds", "nodes", "proxies"])
        .into_iter()
        .filter_map(|outbound| {
            let tag = string_at(&outbound, &["tag", "name", "id"])?;
            let kind = resolve_outbound_protocol(&outbound).to_string();
            Some((tag, kind))
        })
        .collect()
}

