use crate::errors::{AppError, AppResult};
use crate::models::{app_config::AppDnsConfig, dns_config::CLIENT_DNS_DETOUR_ROUTE_FINAL};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// DNS/Fake-IP is a client-owned runtime concern rather than part of a proxy
/// profile. Remove any legacy profile-owned value and inject the persisted
/// global setting into the effective config sent to Zero.
pub(crate) fn apply_global_dns(config: &mut Value, app_dns: &AppDnsConfig) -> AppResult<()> {
    let resolved_dns = if app_dns.enabled {
        let dns = app_dns.config.as_ref().ok_or_else(|| {
            AppError::invalid_argument("global DNS is enabled without a DNS configuration")
        })?;
        let mut dns = serde_json::to_value(dns).map_err(|error| {
            AppError::internal(format!("failed to serialize global DNS config: {error}"))
        })?;
        resolve_dns_detours(config, &mut dns)?;
        Some(dns)
    } else {
        None
    };
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("proxy config must be a JSON object"))?;
    let runtime = root
        .entry("runtime".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let runtime = runtime
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("runtime must be a JSON object"))?;
    runtime.remove("dns");

    if let Some(dns) = resolved_dns {
        runtime.insert("dns".to_string(), dns);
    } else if runtime.is_empty() {
        root.remove("runtime");
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RouteFinalDnsDetour {
    Direct,
    Outbound(String),
}

pub(crate) fn resolve_dns_detours(config: &Value, dns: &mut Value) -> AppResult<()> {
    let route_targets = route_target_tags(config);
    let servers = dns
        .get_mut("servers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::invalid_argument("global DNS servers must be a JSON object"))?;
    let follows_route_final = servers.values().any(|server| {
        server.get("detour").and_then(Value::as_str) == Some(CLIENT_DNS_DETOUR_ROUTE_FINAL)
    });
    let route_final = follows_route_final
        .then(|| route_final_dns_detour(config, &route_targets))
        .transpose()?;

    for (name, server) in servers {
        let Some(server) = server.as_object_mut() else {
            return Err(AppError::invalid_argument(format!(
                "global DNS server `{name}` must be a JSON object"
            )));
        };
        let Some(detour) = server
            .get("detour")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_owned)
        else {
            continue;
        };
        if detour.is_empty() {
            return Err(AppError::invalid_argument(format!(
                "global DNS server `{name}` has an empty detour"
            )));
        }
        if detour == CLIENT_DNS_DETOUR_ROUTE_FINAL {
            match route_final.as_ref().expect("route.final was resolved") {
                RouteFinalDnsDetour::Direct => {
                    server.remove("detour");
                }
                RouteFinalDnsDetour::Outbound(tag) => {
                    server.insert("detour".to_string(), Value::String(tag.clone()));
                }
            }
        } else if !route_targets.contains(&detour) {
            return Err(AppError::invalid_argument(format!(
                "global DNS server `{name}` references undefined detour `{detour}` in the target proxy config"
            )));
        }
    }
    Ok(())
}

fn route_target_tags(config: &Value) -> HashSet<String> {
    let mut targets = HashSet::from(["direct".to_owned(), "block".to_owned()]);
    targets.extend(
        ["outbounds", "outbound_groups"]
            .into_iter()
            .filter_map(|key| config.get(key).and_then(Value::as_array))
            .flatten()
            .filter_map(|target| target.get("tag").and_then(Value::as_str))
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned),
    );
    targets
}

fn route_final_dns_detour(
    config: &Value,
    route_targets: &HashSet<String>,
) -> AppResult<RouteFinalDnsDetour> {
    let final_route = config
        .pointer("/route/final")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::invalid_argument(
                "global DNS detour follows route.final, but the target proxy config has no route.final",
            )
        })?;
    match final_route.get("type").and_then(Value::as_str) {
        Some("direct") => Ok(RouteFinalDnsDetour::Direct),
        Some("route") => {
            let outbound = final_route
                .get("outbound")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .ok_or_else(|| {
                    AppError::invalid_argument(
                        "target proxy config route.final requires a non-empty outbound",
                    )
                })?;
            if !route_targets.contains(outbound) {
                return Err(AppError::invalid_argument(format!(
                    "target proxy config route.final references undefined outbound `{outbound}`"
                )));
            }
            Ok(RouteFinalDnsDetour::Outbound(outbound.to_owned()))
        }
        Some(kind) => Err(AppError::invalid_argument(format!(
            "global DNS detour cannot follow target proxy config route.final of type `{kind}`"
        ))),
        None => Err(AppError::invalid_argument(
            "target proxy config route.final requires a type",
        )),
    }
}

pub(crate) fn strip_profile_dns(config: &mut Value) {
    let Some(root) = config.as_object_mut() else {
        return;
    };
    let remove_runtime = root
        .get_mut("runtime")
        .and_then(Value::as_object_mut)
        .map(|runtime| {
            runtime.remove("dns");
            runtime.is_empty()
        })
        .unwrap_or(false);
    if remove_runtime {
        root.remove("runtime");
    }
}
