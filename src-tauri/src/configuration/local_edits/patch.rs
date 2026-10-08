use super::{migrate_legacy, settings, validate_client_edits};
use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::services::app_config;
use serde_json::{json, Value};

/// Compatibility commands share the same explicit global fields as the editors.
/// Read partial patches against the effective settings, but keep fallback defaults
/// separate so removing an override can restore the source/default value.
pub(crate) fn prepare_patch(
    previous: &AppConfig,
    id: Option<&str>,
    source: &Value,
    patch: AppConfigPatch,
) -> AppResult<AppConfig> {
    let mut saved = previous.clone();
    migrate_legacy(&mut saved, id);
    let tun_parameters = patch.tun.as_ref().is_some_and(|p| {
        p.name.is_some()
            || p.addr.is_some()
            || p.mask.is_some()
            || p.secondary_addr.is_some()
            || p.tag.is_some()
            || p.mtu.is_some()
            || p.include_cidrs.is_some()
            || p.exclude_cidrs.is_some()
            || p.dual_stack.is_some()
            || p.dns_hijack.is_some()
    });
    let network = patch.local_proxy.is_some()
        || tun_parameters
        || patch.dns.is_some()
        || patch.bypass.is_some()
        || patch.url_test.is_some()
        || patch.routing.is_some()
        || patch.runtime.is_some();
    if !network {
        return app_config::prepare_update(&saved, patch);
    }
    let effective: AppConfig = serde_json::from_value(settings(&saved, id, source)?)
        .map_err(|error| AppError::invalid_argument(error.to_string()))?;
    let normalized = app_config::prepare_update(&effective, patch.clone())?;
    let values = json!(normalized);
    let mut next = normalized.clone();
    if patch.overrides.is_none() {
        next.overrides = saved.overrides.clone();
    }
    // These are defaults, not the explicit settings store.
    next.local_proxy = saved.local_proxy.clone();
    next.tun = saved.tun.clone();
    next.dns = saved.dns.clone();
    next.url_test = saved.url_test.clone();
    next.runtime = saved.runtime.clone();
    next.routing = saved.routing.clone();
    next.bypass = saved.bypass.clone();
    let edits = next.client_edits.as_mut().unwrap();
    macro_rules! field {
        ($section:literal, $key:literal, $present:expr) => {
            if $present {
                edits.insert(
                    concat!($section, ".", $key).into(),
                    values[$section][$key].clone(),
                );
            }
        };
    }
    if let Some(p) = &patch.local_proxy {
        field!("localProxy", "host", p.host.is_some());
        field!("localProxy", "port", p.port.is_some());
        if p.source_proxy_config_id.is_some() {
            next.local_proxy.source_proxy_config_id = normalized.local_proxy.source_proxy_config_id;
        }
    }
    if let Some(p) = &patch.tun {
        field!("tun", "name", p.name.is_some());
        field!("tun", "tag", p.tag.is_some());
        field!("tun", "addr", p.addr.is_some());
        field!("tun", "secondaryAddr", p.secondary_addr.is_some());
        field!("tun", "mtu", p.mtu.is_some());
        field!("tun", "includeCidrs", p.include_cidrs.is_some());
        field!("tun", "excludeCidrs", p.exclude_cidrs.is_some());
        field!("tun", "dualStack", p.dual_stack.is_some());
        field!("tun", "dnsHijack", p.dns_hijack.is_some());
        // Enabled is the global capture intent, never a subscription override.
        if p.enabled.is_some() {
            next.tun.enabled = normalized.tun.enabled;
        }
    }
    if let Some(p) = &patch.url_test {
        field!("urlTest", "url", p.url.is_some());
        field!("urlTest", "toleranceMs", p.tolerance_ms.is_some());
    }
    if let Some(p) = &patch.runtime {
        field!(
            "runtime",
            "udpUpstreamIdleTimeoutSeconds",
            p.udp_upstream_idle_timeout_seconds.is_some()
        );
    }
    if let Some(p) = &patch.routing {
        field!(
            "routing",
            "injectCommonRules",
            p.inject_common_rules.is_some()
        );
    }
    if patch.dns.is_some() {
        edits.insert("dns".into(), values["dns"].clone());
    }
    let legacy_bypass = patch
        .local_proxy
        .as_ref()
        .is_some_and(|p| p.bypass.is_some())
        || patch
            .tun
            .as_ref()
            .is_some_and(|p| p.exclude_cidrs.is_some());
    if patch.bypass.is_some() || legacy_bypass {
        edits.insert("bypass".into(), values["bypass"].clone());
    }
    validate_client_edits(&next, next.client_edits.as_ref().unwrap())?;
    Ok(next)
}

#[cfg(test)]
#[path = "patch_tests.rs"]
mod tests;
