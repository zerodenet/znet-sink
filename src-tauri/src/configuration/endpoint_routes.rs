//! Endpoint bindings use the ordinary route compiler and retain source policy.
use crate::errors::{AppError, AppResult};
use crate::models::rule_set::{CommonRuleAction, RuleSetProfile};
use serde_json::Value;
use std::net::IpAddr;

fn network(value: &str) -> Option<(u128, u32, u32)> {
    let (address, prefix) = value.split_once('/').unwrap_or((value, ""));
    let ip: IpAddr = address.parse().ok()?;
    let bits = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = if prefix.is_empty() {
        bits
    } else {
        prefix.parse().ok()?
    };
    if prefix > bits {
        return None;
    }
    let address = match ip {
        IpAddr::V4(ip) => u32::from(ip) as u128,
        IpAddr::V6(ip) => u128::from(ip),
    };
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (bits - prefix)
    };
    Some((address & mask, prefix, bits))
}
fn covers(parent: &str, child: &str) -> bool {
    let (Some((a, p, bits)), Some((b, q, other))) = (network(parent), network(child)) else {
        return false;
    };
    let mask = if p == 0 { 0 } else { u128::MAX << (bits - p) };
    bits == other && p <= q && a == b & mask
}
pub(crate) fn validate_target(base: &Value, tag: &str, ir: &Value) -> AppResult<()> {
    let endpoint = base["endpoints"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["tag"] == tag))
        .ok_or_else(|| {
            AppError::invalid_argument(format!("路由引用的端点 {tag} 不存在，请重新选择端点"))
        })?;
    if endpoint["enabled"] == false
        || endpoint.pointer("/directions/outbound") == Some(&Value::Bool(false))
    {
        return Err(AppError::invalid_argument(format!(
            "端点 {tag} 未启用出站，请先启用或停用引用它的路由规则"
        )));
    }
    if endpoint.pointer("/protocol/type").and_then(Value::as_str) == Some("wireguard") {
        let allowed: Vec<&str> = endpoint
            .pointer("/protocol/peers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|p| p["allowed_ips"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect();
        for rule in ir["rules"].as_array().into_iter().flatten() {
            if matches!(rule["type"].as_str(), Some("ipv4_cidr" | "ipv6_cidr")) {
                let value = rule["value"].as_str().unwrap_or("");
                if !allowed.iter().any(|n| covers(n, value)) {
                    return Err(AppError::invalid_argument(format!(
                        "端点 {tag} 的 AllowedIPs 未完整覆盖 {value}；请调整规则或端点允许网段"
                    )));
                }
            }
        }
    }
    Ok(())
}
/// Native/TUN exclusions precede route evaluation. Never report such a binding
/// as applied successfully: a domain may resolve to any excluded address.
pub(crate) fn validate_bypass(config: &Value, profiles: &[RuleSetProfile]) -> AppResult<()> {
    for profile in super::rules::eligible_profiles(profiles) {
        let Some(CommonRuleAction::Endpoint(tag)) =
            profile.common_binding.as_ref().map(|b| &b.action)
        else {
            continue;
        };
        // Only bindings actually emitted in rule mode participate.
        let emitted = config
            .pointer("/route/rules")
            .and_then(Value::as_array)
            .is_some_and(|rules| {
                rules.iter().any(|r| {
                    r.pointer("/condition/tag").and_then(Value::as_str)
                        == Some(super::rules::common_tag(&profile.id).as_str())
                })
            });
        if !emitted {
            continue;
        }
        for bypass in config
            .pointer("/route/bypass")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for value in bypass["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if profile.semantic_ir["rules"]
                    .as_array()
                    .is_none_or(|rules| rules.is_empty())
                {
                    return Err(AppError::invalid_argument(format!("规则“{}”的外部规则内容无法静态确认是否与提前绕过 {value} 重叠；请改用明确的本地域名或网段规则后再选择端点 {tag}",profile.name)));
                }
                let conflict = if bypass["type"] == "ip" {
                    profile.semantic_ir["rules"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|r| match r["type"].as_str() {
                            Some("ipv4_cidr" | "ipv6_cidr") => {
                                let target = r["value"].as_str().unwrap_or("");
                                covers(value, target) || covers(target, value)
                            }
                            Some(_) => {
                                !covers("127.0.0.0/8", value)
                                    && !covers("::1/128", value)
                                    && value != "::/128"
                            }
                            None => false,
                        })
                } else {
                    // Native domain exceptions cannot express an ordered exception.
                    profile.semantic_ir["rules"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|r| {
                            r["type"].as_str().is_some_and(|t| t.starts_with("domain"))
                                && r["value"].as_str().is_some_and(|v| {
                                    regex::Regex::new(value).is_ok_and(|re| {
                                        re.is_match(v)
                                            || (r["type"] == "domain_suffix"
                                                && (re.is_match(&format!("probe.{v}"))
                                                    || value.contains(&v.replace('.', "\\."))))
                                    })
                                })
                        })
                };
                if conflict {
                    return Err(AppError::invalid_argument(format!("规则“{}” → {tag} 与提前绕过 {value} 冲突。请先在网络设置调整绕过，再应用；IP 绕过可改为排在端点规则之后的普通直连规则",profile.name)));
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "endpoint_routes_tests.rs"]
mod tests;

/// Command-managed TUN exclusions are not retained in the emitted kernel
/// configuration. Validate the actual client capture plan as well as core bypass.
pub(crate) fn validate_capture(
    config: &Value,
    source: &Value,
    app: &crate::models::app_config::AppConfig,
    profiles: &[RuleSetProfile],
) -> AppResult<()> {
    if !super::rules::eligible_profiles(profiles).any(|p| {
        matches!(
            p.common_binding.as_ref().map(|b| &b.action),
            Some(CommonRuleAction::Endpoint(_))
        )
    }) {
        return Ok(());
    }
    validate_bypass(config, profiles)?;
    let params = super::preferences::tun_params_for(app, source, app.tun.clone())?;
    let mut capture = config.clone();
    if let Some(networks) = params["exclude_cidrs"].as_array() {
        if !networks.is_empty() {
            let mut bypass = config
                .pointer("/route/bypass")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            bypass.push(serde_json::json!({"type":"ip","values":networks}));
            capture["route"]["bypass"] = serde_json::json!(bypass);
        }
    }
    validate_bypass(&capture, profiles)
}
