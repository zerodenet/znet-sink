//! Zero runtime surfaces introduced by the v0.0.16 development line.
//!
//! These are kept separate from the generic `KernelAdapter` feature status so
//! Zero-specific route/interface metadata does not get flattened into a
//! boolean capability flag.

use serde_json::{json, Map, Value};

use crate::errors::AppResult;
use crate::models::app_config::AppTunConfig;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::GuiFeatureStatus;
use crate::models::zero_runtime::GuiTunStatus;

use super::{commands, parsing, queries};

pub async fn tun_status(options: Option<CoreIpcOptions>) -> AppResult<GuiTunStatus> {
    let fallback = queries::feature_status(
        "tun",
        &["tun", "tun-status", "tun-snapshot"],
        options.clone(),
    )
    .await
    .ok();

    match queries::query_value(json!({"tun_status": {}}), "tun_status", options).await {
        Ok(value) => Ok(parse_tun_status(&value, fallback.as_ref())),
        Err(error) => fallback.map(from_feature_status).ok_or(error),
    }
}

pub async fn enable_tun(
    tun: AppTunConfig,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiTunStatus> {
    // Capability/state is authoritative for whether the client should prepare
    // platform dependencies. An unsupported or temporarily unreadable Zero
    // must never cause a speculative Wintun download.
    let status = tun_status(options.clone()).await?;
    if status.enabled || !status.supported {
        return Ok(status);
    }

    #[cfg(windows)]
    super::wintun_compat::ensure_for_current_runtime().await?;

    let params = build_tun_start_params(tun);
    let response = commands::run_command("tun.start", params, options.clone()).await?;
    let fallback = parsing::parse_feature_runtime_status("tun", &response, None);

    match tun_status(options).await {
        Ok(status) => Ok(status),
        Err(_) => Ok(from_feature_status(fallback)),
    }
}

pub async fn disable_tun(options: Option<CoreIpcOptions>) -> AppResult<GuiTunStatus> {
    if let Ok(status) = tun_status(options.clone()).await {
        if !status.enabled {
            return Ok(status);
        }
    }

    let response = commands::run_command("tun.stop", json!({}), options.clone()).await?;
    let fallback = parsing::parse_feature_runtime_status("tun", &response, None);

    match tun_status(options).await {
        Ok(status) => Ok(status),
        Err(_) => Ok(from_feature_status(fallback)),
    }
}

fn build_tun_start_params(tun: AppTunConfig) -> Value {
    let mut params = Map::new();
    if let Some(name) = tun.name {
        params.insert("name".to_string(), json!(name));
    }
    params.insert("addr".to_string(), json!(tun.addr));
    params.insert("mask".to_string(), json!(tun.mask));
    if tun.dual_stack {
        if let Some(secondary_addr) = tun.secondary_addr {
            params.insert("secondary_addr".to_string(), json!(secondary_addr));
        }
    }
    params.insert("tag".to_string(), json!(tun.tag));
    params.insert("mtu".to_string(), json!(tun.mtu));

    // ZNet-Sink's command-managed TUN mode means full system capture. These
    // are Zero runtime parameters, but they are client policy rather than user
    // preferences: route installation must be attempted and failure must roll
    // the activation back instead of leaving a half-captured session.
    params.insert("auto_route".to_string(), json!(true));
    params.insert("strict_route".to_string(), json!(true));
    params.insert("dual_stack".to_string(), json!(tun.dual_stack));

    // DNS hijack is only user-selectable after the DNS configuration surface
    // has validated and persisted runtime.dns. The frontend guards the
    // precondition and Zero remains the final authority at tun.start.
    params.insert("dns_hijack".to_string(), json!(tun.dns_hijack));
    Value::Object(params)
}

fn parse_tun_status(value: &Value, fallback: Option<&GuiFeatureStatus>) -> GuiTunStatus {
    let running = bool_at(value, &["running", "enabled"])
        .unwrap_or_else(|| fallback.is_some_and(|status| status.enabled));
    let healthy = bool_at(value, &["healthy"]).unwrap_or(running);
    let last_error = parsing::string_at(value, &["last_error", "lastError", "error"]);
    // A successful typed tun_status query is itself proof that the runtime
    // surface is supported. Capability metadata is only a fallback path for
    // older Zero builds and must not override a successful query.
    let supported = true;
    let state = if !healthy && running {
        "error"
    } else if running {
        "running"
    } else {
        "stopped"
    }
    .to_string();

    GuiTunStatus {
        key: "tun".to_string(),
        supported,
        enabled: running,
        state,
        reason: last_error
            .clone()
            .or_else(|| fallback.and_then(|status| status.reason.clone())),
        name: parsing::string_at(value, &["name", "interface_name", "interfaceName"]),
        addr: parsing::string_at(value, &["addr", "address"]),
        addresses: string_array_at(value, &["addresses"]),
        mtu: parsing::u64_at(value, &["mtu"]).and_then(|mtu| u16::try_from(mtu).ok()),
        tag: parsing::string_at(value, &["tag"]),
        healthy,
        auto_route: bool_at(value, &["auto_route", "autoRoute"]).unwrap_or(false),
        dual_stack: bool_at(value, &["dual_stack", "dualStack"]).unwrap_or(false),
        strict_route: bool_at(value, &["strict_route", "strictRoute"]).unwrap_or(false),
        dns_hijack: bool_at(value, &["dns_hijack", "dnsHijack"]).unwrap_or(false),
        egress_interface: parsing::string_at(value, &["egress_interface", "egressInterface"]),
        egress_interface_v4: parsing::string_at(
            value,
            &["egress_interface_v4", "egressInterfaceV4"],
        ),
        egress_interface_v6: parsing::string_at(
            value,
            &["egress_interface_v6", "egressInterfaceV6"],
        ),
        ipv4_egress: parse_tun_family_egress(
            value,
            &["ipv4_egress", "ipv4Egress"],
            &["egress_interface_v4", "egressInterfaceV4"],
        ),
        ipv6_egress: parse_tun_family_egress(
            value,
            &["ipv6_egress", "ipv6Egress"],
            &["egress_interface_v6", "egressInterfaceV6"],
        ),
        network_generation: parsing::u64_at(value, &["network_generation", "networkGeneration"])
            .unwrap_or(0),
        address_family_policy: parsing::string_at(
            value,
            &["address_family_policy", "addressFamilyPolicy"],
        ),
        ipv6_to_ipv4_fallbacks: parsing::u64_at(
            value,
            &["ipv6_to_ipv4_fallbacks", "ipv6ToIpv4Fallbacks"],
        )
        .unwrap_or(0),
        last_error,
        managed_by_config: bool_at(value, &["managed_by_config", "managedByConfig"])
            .unwrap_or(false),
    }
}

fn parse_tun_family_egress(
    value: &Value,
    keys: &[&str],
    legacy_interface_keys: &[&str],
) -> crate::models::zero_runtime::GuiTunFamilyEgress {
    let family = keys.iter().find_map(|key| value.get(*key));
    let interface = family
        .and_then(|family| parsing::string_at(family, &["interface"]))
        .or_else(|| parsing::string_at(value, legacy_interface_keys));
    let reason = family.and_then(|family| parsing::string_at(family, &["reason"]));
    let availability = family
        .and_then(|family| parsing::string_at(family, &["availability", "state"]))
        .unwrap_or_else(|| {
            if interface.is_some() {
                "available".to_string()
            } else {
                "unknown".to_string()
            }
        });
    crate::models::zero_runtime::GuiTunFamilyEgress {
        availability,
        interface,
        reason,
    }
}

fn from_feature_status(status: GuiFeatureStatus) -> GuiTunStatus {
    GuiTunStatus {
        key: status.key,
        supported: status.supported,
        enabled: status.enabled,
        state: status.state,
        reason: status.reason,
        healthy: status.enabled,
        ..GuiTunStatus::default()
    }
}

fn bool_at(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn string_array_at(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::models::app_config::AppTunConfig;

    use super::{build_tun_start_params, parse_tun_status};

    #[test]
    fn tun_start_uses_full_capture_policy_and_persisted_interface_values() {
        let tun = AppTunConfig {
            enabled: Some(true),
            name: Some("CustomTun".to_string()),
            addr: "10.88.0.1/24".to_string(),
            mask: "255.255.255.0".to_string(),
            secondary_addr: Some("fd88::1/64".to_string()),
            tag: "tun-in".to_string(),
            mtu: 1400,
            dual_stack: true,
            dns_hijack: true,
        };
        let params = build_tun_start_params(tun);

        assert_eq!(params["name"], "CustomTun");
        assert_eq!(params["addr"], "10.88.0.1/24");
        assert_eq!(params["secondary_addr"], "fd88::1/64");
        assert_eq!(params["tag"], "tun-in");
        assert_eq!(params["mtu"], 1400);
        assert_eq!(params["auto_route"], true);
        assert_eq!(params["strict_route"], true);
        assert_eq!(params["dual_stack"], true);
        assert_eq!(params["dns_hijack"], true);
    }

    #[test]
    fn single_stack_keeps_saved_secondary_out_of_the_start_payload() {
        let tun = AppTunConfig {
            secondary_addr: Some("fd88::1/64".to_string()),
            dual_stack: false,
            ..AppTunConfig::default()
        };
        let params = build_tun_start_params(tun);

        assert!(params.get("secondary_addr").is_none());
        assert_eq!(params["dual_stack"], false);
    }

    #[test]
    fn parses_detailed_tun_route_state() {
        let status = parse_tun_status(
            &json!({
                "running": true,
                "healthy": true,
                "name": "znet0",
                "addr": "10.66.0.1/24",
                "addresses": ["10.66.0.1/24", "fd66::1/64"],
                "mtu": 1500,
                "tag": "tun",
                "auto_route": true,
                "dual_stack": true,
                "strict_route": true,
                "dns_hijack": false,
                "egress_interface_v4": "Ethernet",
                "egress_interface_v6": "Ethernet",
                "ipv4_egress": {
                    "availability": "available",
                    "interface": "Ethernet"
                },
                "ipv6_egress": {
                    "availability": "unavailable",
                    "reason": "no_default_route"
                },
                "network_generation": 7,
                "address_family_policy": "prefer_ipv4",
                "ipv6_to_ipv4_fallbacks": 2,
                "managed_by_config": false
            }),
            None,
        );

        assert!(status.supported);
        assert!(status.enabled);
        assert_eq!(status.addresses.len(), 2);
        assert!(status.auto_route);
        assert!(status.dual_stack);
        assert!(status.strict_route);
        assert!(!status.dns_hijack);
        assert_eq!(status.egress_interface_v4.as_deref(), Some("Ethernet"));
        assert_eq!(status.ipv4_egress.availability, "available");
        assert_eq!(status.ipv4_egress.interface.as_deref(), Some("Ethernet"));
        assert_eq!(status.ipv6_egress.availability, "unavailable");
        assert_eq!(
            status.ipv6_egress.reason.as_deref(),
            Some("no_default_route")
        );
        assert_eq!(status.network_generation, 7);
        assert_eq!(status.address_family_policy.as_deref(), Some("prefer_ipv4"));
        assert_eq!(status.ipv6_to_ipv4_fallbacks, 2);
    }
}
