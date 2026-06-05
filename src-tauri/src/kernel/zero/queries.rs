//! Zero kernel query methods.
//!
//! Each function sends an IPC query and parses the response into a
//! GUI model type. All functions are stateless — they receive
//! `CoreIpcOptions` and return parsed results.

use serde_json::{json, Map, Value};

use crate::errors::AppResult;
use crate::kernel::protocol;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::{
    GuiConnection, GuiConnectionList, GuiConnectionListOptions, GuiCoreHealth,
    GuiFeatureStatus, GuiPolicyGroup, GuiTrafficStats, GuiZeroCapabilities,
};

use super::parsing::{
    normalize_optional, parse_capabilities, parse_connection, parse_connection_list,
    parse_feature_runtime_status, parse_health, parse_policy_groups, parse_stats,
    unwrap_call_result,
};

const DEFAULT_CONNECTION_LIMIT: u32 = 100;
const MAX_CONNECTION_LIMIT: u32 = 500;

/// Query detailed kernel health (version, uptime, etc.).
pub async fn core_health(options: Option<CoreIpcOptions>) -> AppResult<GuiCoreHealth> {
    let value = query_result(json!({"health": {}}), options).await?;
    Ok(parse_health(&value))
}

/// Fast liveness check — ping first, then optionally enrich with health.
pub async fn core_readiness_health(options: Option<CoreIpcOptions>) -> AppResult<GuiCoreHealth> {
    let ping = protocol::ping(options.clone()).await?;
    unwrap_call_result(ping.response, ping.error)?;

    match query_result_with_timeout(json!({"health": {}}), 2_000, options).await {
        Ok(value) => Ok(parse_health(&value)),
        Err(_) => Ok(GuiCoreHealth {
            healthy: true,
            engine_version: None,
            started_at_unix_ms: None,
        }),
    }
}

/// Query traffic counters.
pub async fn traffic_stats(options: Option<CoreIpcOptions>) -> AppResult<GuiTrafficStats> {
    let value = query_result(json!({"stats": {}}), options).await?;
    Ok(parse_stats(&value))
}

/// Query kernel capabilities surface.
pub async fn zero_capabilities(options: Option<CoreIpcOptions>) -> AppResult<GuiZeroCapabilities> {
    let value = query_result(json!({"capabilities": {}}), options).await?;
    Ok(parse_capabilities(&value, None))
}

/// Extract feature keys from capabilities.
pub async fn capability_feature_keys(options: Option<CoreIpcOptions>) -> AppResult<Vec<String>> {
    Ok(zero_capabilities(options).await?.features)
}

/// Query all policy groups.
pub async fn policy_groups(options: Option<CoreIpcOptions>) -> AppResult<Vec<GuiPolicyGroup>> {
    let value = query_result(json!({"policies": {}}), options).await?;
    Ok(parse_policy_groups(&value))
}

/// Query active connections (flows).
pub async fn connections(
    list_options: Option<GuiConnectionListOptions>,
    ipc_options: Option<CoreIpcOptions>,
) -> AppResult<GuiConnectionList> {
    let options = list_options.unwrap_or(GuiConnectionListOptions {
        limit: None,
        inbound_tag: None,
        principal_key: None,
    });
    let limit = options
        .limit
        .unwrap_or(DEFAULT_CONNECTION_LIMIT)
        .clamp(1, MAX_CONNECTION_LIMIT);
    let mut filter = Map::new();
    if let Some(inbound_tag) = normalize_optional(options.inbound_tag) {
        filter.insert("inbound_tag".to_string(), json!(inbound_tag));
    }
    if let Some(principal_key) = normalize_optional(options.principal_key) {
        filter.insert("principal_key".to_string(), json!(principal_key));
    }

    let value = query_result(
        json!({
            "active_flows": {
                "limit": limit,
                "filter": Value::Object(filter),
            }
        }),
        ipc_options,
    )
    .await?;

    Ok(parse_connection_list(&value, limit))
}

/// Query a single connection by flow ID.
pub async fn connection_detail(
    flow_id: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiConnection> {
    let flow_id = super::parsing::normalize_non_empty(flow_id, "flowId")?;
    let value = query_result(json!({"flow": {"flow_id": flow_id}}), options).await?;
    parse_connection(&value)
        .ok_or_else(|| crate::errors::AppError::invalid_argument("core returned invalid flow"))
}

/// Check if a feature is supported by querying capabilities.
pub async fn feature_status(
    key: &str,
    candidates: &[&str],
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiFeatureStatus> {
    let capabilities = zero_capabilities(options).await?;
    let supported = capabilities.features.iter().any(|feature| {
        candidates
            .iter()
            .any(|candidate| feature.eq_ignore_ascii_case(candidate))
    });

    Ok(GuiFeatureStatus {
        key: key.to_string(),
        supported,
        enabled: false,
        state: if supported {
            "available"
        } else {
            "unsupported"
        }
        .to_string(),
        reason: (!supported).then(|| format!("zero capability does not declare {key}")),
    })
}

/// DNS subsystem status.
pub async fn dns_status(options: Option<CoreIpcOptions>) -> AppResult<GuiFeatureStatus> {
    feature_status("dns", &["dns", "dns-status", "dns-snapshot"], options).await
}

/// TUN status with runtime fallback queries.
pub async fn tun_status(options: Option<CoreIpcOptions>) -> AppResult<GuiFeatureStatus> {
    let fallback = feature_status("tun", &["tun", "tun-status", "tun-snapshot"], options.clone()).await;

    if let Ok(value) = query_result(json!({"tun_status": {}}), options.clone()).await {
        return Ok(parse_feature_runtime_status(
            "tun",
            &value,
            fallback.as_ref().ok(),
        ));
    }

    if let Ok(value) =
        super::commands::run_command("tun.status", json!({}), options).await
    {
        return Ok(parse_feature_runtime_status(
            "tun",
            &value,
            fallback.as_ref().ok(),
        ));
    }

    fallback
}

/// Network stack status.
pub async fn stack_status(options: Option<CoreIpcOptions>) -> AppResult<GuiFeatureStatus> {
    feature_status(
        "stack",
        &["system-stack", "stack", "stack-status"],
        options,
    )
    .await
}

/// Rule engine status.
pub async fn rule_status(options: Option<CoreIpcOptions>) -> AppResult<GuiFeatureStatus> {
    feature_status(
        "rules",
        &["rules", "rule-status", "rule-snapshot", "routing"],
        options,
    )
    .await
}

// ── Internal helpers ────────────────────────────────────────────────

async fn query_result(
    request: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let call = protocol::query(request, options).await?;
    unwrap_call_result(call.response, call.error)
}

async fn query_result_with_timeout(
    request: Value,
    timeout_ms: u64,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let mut opts = options.unwrap_or_default();
    opts.timeout_ms = Some(timeout_ms);
    let call = protocol::query(request, Some(opts)).await?;
    unwrap_call_result(call.response, call.error)
}
