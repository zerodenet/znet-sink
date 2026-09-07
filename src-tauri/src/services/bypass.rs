//! One direct-access policy projected into native proxy exceptions, TUN CIDRs
//! and mode-independent core routing. No network access or DNS lookups here.

use serde_json::{json, Value};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::{default_proxy_bypass, AppBypassConfig, AppConfig};

mod rules;
use rules::{domain_pattern, native_patterns, network};

const LOCAL_NETWORKS: &[&str] = &[
    "127.0.0.0/8",
    "::1/128",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "fe80::/10",
    "fc00::/7",
];

pub fn normalize(config: &mut AppConfig) -> AppResult<()> {
    let policy = config.bypass.clone().unwrap_or_else(|| {
        let defaults = default_proxy_bypass();
        let local_networks = defaults
            .iter()
            .all(|rule| config.local_proxy.bypass.contains(rule));
        let mut rules: Vec<_> = config
            .local_proxy
            .bypass
            .iter()
            .filter(|rule| !local_networks || !defaults.contains(rule))
            .cloned()
            .collect();
        rules.extend(config.tun.exclude_cidrs.clone());
        AppBypassConfig {
            local_networks,
            rules,
        }
    });
    let mut rules = Vec::new();
    for rule in policy.rules {
        let rule = rule.trim().to_ascii_lowercase();
        if rule.is_empty() {
            continue;
        }
        let normalized = if let Some(network) = network(&rule)? {
            network
        } else {
            domain_pattern(&rule)?;
            rule
        };
        if !rules.contains(&normalized) {
            rules.push(normalized);
        }
    }
    config.bypass = Some(AppBypassConfig {
        local_networks: policy.local_networks,
        rules,
    });
    let entries = entries(config);
    config.tun.exclude_cidrs = entries
        .iter()
        .filter_map(|rule| network(rule).transpose())
        .collect::<AppResult<_>>()?;
    if !config.tun.dual_stack {
        let ipv6 = config.tun.addr.contains(':');
        config
            .tun
            .exclude_cidrs
            .retain(|network| network.contains(':') == ipv6);
    }
    // Arbitrary CIDRs unsupported by native proxy APIs remain covered by the
    // core's direct-access policy instead of widening the native exception.
    config.local_proxy.bypass = entries
        .iter()
        .flat_map(|rule| native_patterns(rule))
        .collect();
    Ok(())
}

fn entries(config: &AppConfig) -> Vec<String> {
    let Some(policy) = &config.bypass else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    if policy.local_networks {
        entries.extend(LOCAL_NETWORKS.iter().map(|v| v.to_string()));
        entries.extend(["localhost".into(), "*.local".into(), "<local>".into()]);
    }
    for rule in &policy.rules {
        if !entries.contains(rule) {
            entries.push(rule.clone());
        }
    }
    entries
}

/// Compare both address families: a profile-owned TUN can use a different
/// family from the client's default TUN. Callers pass normalized settings.
pub(crate) fn network_rules_changed(old: &AppConfig, next: &AppConfig) -> bool {
    let networks = |app: &AppConfig| -> Vec<String> {
        entries(app)
            .into_iter()
            .filter_map(|rule| network(&rule).ok().flatten())
            .collect()
    };
    networks(old) != networks(next)
}

pub fn apply(config: &mut Value, app: &AppConfig) -> AppResult<()> {
    let mut app = app.clone();
    normalize(&mut app)?;
    let conditions: Vec<_> = entries(&app)
        .iter()
        .map(|rule| {
            if let Some(network) = network(rule)? {
                Ok(json!({"type":"ip", "values":[network]}))
            } else {
                Ok(json!({"type":"domain_regex", "values":[domain_pattern(rule)?]}))
            }
        })
        .collect::<AppResult<_>>()?;
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("proxy config must be an object"))?;
    let route = root
        .entry("route")
        .or_insert_with(|| json!({"final":{"type":"direct"}}));
    let route = route
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("route must be an object"))?;
    // Profile-owned explicit exceptions are additive; source config is never edited.
    if !conditions.is_empty() {
        let bypass = route
            .entry("bypass")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| AppError::invalid_argument("route.bypass must be an array"))?;
        for condition in conditions {
            if !bypass.contains(&condition) {
                bypass.push(condition);
            }
        }
    }
    if let Some(tun) = root
        .get_mut("runtime")
        .and_then(|v| v.get_mut("tun"))
        .and_then(Value::as_object_mut)
    {
        let dual_stack = tun
            .get("dual_stack")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let ipv6 = tun
            .get("addr")
            .and_then(Value::as_str)
            .is_some_and(|addr| addr.contains(':'));
        let exclusions = tun
            .entry("exclude_cidrs")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                AppError::invalid_argument("runtime.tun.exclude_cidrs must be an array")
            })?;
        for rule in entries(&app) {
            let Some(network) = network(&rule)? else {
                continue;
            };
            if !dual_stack && network.contains(':') != ipv6 {
                continue;
            }
            let value = json!(network);
            if !exclusions.contains(&value) {
                exclusions.push(value);
            }
        }
    }
    apply_dns(
        root.get_mut("runtime").and_then(|v| v.get_mut("dns")),
        &entries(&app),
    )?;
    Ok(())
}

// Keep domain exceptions on native DNS while retaining Fake-IP ownership or
// real-IP reverse mappings used to recover the hostname for TUN routing.
fn apply_dns(dns: Option<&mut Value>, entries: &[String]) -> AppResult<()> {
    let Some(dns) = dns.and_then(Value::as_object_mut) else {
        return Ok(());
    };
    let patterns: Vec<_> = entries
        .iter()
        .filter(|rule| network(rule).ok().flatten().is_none())
        .map(|rule| domain_pattern(rule))
        .collect::<AppResult<_>>()?;
    if patterns.is_empty() {
        return Ok(());
    }
    let servers = dns
        .get_mut("servers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::invalid_argument("DNS servers must be an object"))?;
    let server = if let Some((name, _)) = servers
        .iter()
        .find(|(_, v)| v.get("type").and_then(Value::as_str) == Some("system"))
    {
        name.clone()
    } else {
        let mut name = "znet-bypass-system".to_string();
        while servers.contains_key(&name) {
            name.push('-');
        }
        servers.insert(name.clone(), json!({"type":"system"}));
        name
    };
    let dispatch = dns
        .entry("dispatch")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| AppError::invalid_argument("DNS dispatch must be an array"))?;
    let rule = json!({"condition":{"type":"domain_regex","values":patterns},"server":server});
    dispatch.retain(|v| v != &rule);
    dispatch.insert(0, rule);
    dns.entry("reverse_mapping").or_insert_with(|| json!({}));
    Ok(())
}

/// Confirm compatibility before any configuration publication or core restart.
pub async fn require_core_support(app: &AppConfig) -> AppResult<()> {
    if entries(app).is_empty() {
        return Ok(());
    }
    let options = super::core_config::ipc_options_from_app_config(&app.core);
    let features = crate::kernel::zero::queries::capability_feature_keys(Some(options)).await?;
    if features.iter().any(|feature| feature == "route_bypass_v1") {
        return Ok(());
    }
    Err(AppError::invalid_argument(
        "当前内核不支持统一绕过规则，请先更新内核；原配置未修改",
    ))
}

#[cfg(test)]
#[path = "bypass_tests.rs"]
mod tests;
