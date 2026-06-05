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
            let protocol = node
                .get("type")
                .or_else(|| node.get("protocol"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            Some(ConfigProxyNode {
                tag: tag.to_string(),
                protocol: protocol.to_string(),
                is_selector: protocol.eq_ignore_ascii_case("selector"),
            })
        })
        .collect()
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
        tag,
        kind: string_at(&value, &["type", "kind", "policy_kind", "policyKind"])
            .unwrap_or_else(|| "selector".to_string()),
        selected,
        members,
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

fn outbound_kind_map(value: &Value) -> std::collections::HashMap<String, String> {
    values_from_container(value, &["outbounds", "nodes", "proxies"])
        .into_iter()
        .filter_map(|outbound| {
            Some((
                string_at(&outbound, &["tag", "name", "id"])?,
                string_at(&outbound, &["type", "protocol", "kind"])
                    .unwrap_or_else(|| "unknown".to_string()),
            ))
        })
        .collect()
}
