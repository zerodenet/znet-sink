//! Zero kernel command methods.
//!
//! Each function sends an IPC command and parses the response into a
//! GUI model type. Stateless — receives `CoreIpcOptions` directly.

use serde_json::{json, Map, Value};

use crate::errors::AppResult;
use crate::kernel::protocol;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::{
    GuiConfigPlanApplyResult, GuiConnectionCloseResult, GuiFeatureStatus, GuiPolicySelectionResult,
    GuiTargetProbeResult,
};

use super::parsing::{
    normalize_non_empty, parse_connection_close, parse_feature_runtime_status,
    parse_plan_apply_result, parse_policy_selection, parse_target_probe, unwrap_call_result,
};

/// Switch the selected outbound in a policy group.
pub async fn select_policy(
    policy_tag: String,
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiPolicySelectionResult> {
    let policy_tag = normalize_non_empty(policy_tag, "policyTag")?;
    let target_tag = normalize_non_empty(target_tag, "targetTag")?;
    // Reject manual selection for auto-selecting group types
    // (url-test / fallback / load-balance). Only "selector" groups honor a
    // user-picked outbound; in other types the kernel silently ignores it.
    let groups = super::queries::policy_groups(options.clone())
        .await
        .unwrap_or_default();
    if let Some(group) = groups.iter().find(|g| g.name == policy_tag) {
        if !group.kind.eq_ignore_ascii_case("selector") {
            return Err(crate::errors::AppError::invalid_argument(format!(
                "group '{}' is type '{}' — only selector groups support manual selection",
                policy_tag, group.kind
            )));
        }
    }
    let value = run_command(
        "policies.select",
        json!({
            "policy_tag": policy_tag,
            "target_tag": target_tag,
        }),
        options,
    )
    .await?;

    Ok(parse_policy_selection(&value, policy_tag, target_tag))
}

/// Probe a url_test policy group (triggers latency measurement).
pub async fn probe_policy(policy_tag: String, options: Option<CoreIpcOptions>) -> AppResult<Value> {
    let policy_tag = normalize_non_empty(policy_tag, "policyTag")?;
    run_command(
        "policies.probe",
        json!({ "policy_tag": policy_tag }),
        options,
    )
    .await
}

/// Probe a single target for reachability and latency.
pub async fn probe_target(
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiTargetProbeResult> {
    let target_tag = normalize_non_empty(target_tag, "targetTag")?;
    let value = run_command(
        "diagnostics.probe_target",
        json!({ "target_tag": target_tag }),
        options,
    )
    .await?;

    Ok(parse_target_probe(&value, target_tag))
}

/// Close an active flow.
pub async fn close_connection(
    flow_id: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiConnectionCloseResult> {
    let flow_id = normalize_non_empty(flow_id, "flowId")?;
    let value = run_command("flows.close", json!({ "flow_id": flow_id }), options).await?;
    Ok(parse_connection_close(&value, flow_id))
}

/// Hot-apply a full config without restarting the kernel.
pub async fn apply_config(config: Value, options: Option<CoreIpcOptions>) -> AppResult<Value> {
    if !config.is_object() {
        return Err(crate::errors::AppError::invalid_argument(
            "config must be a JSON object",
        ));
    }
    run_command("config.apply", json!({ "config": config }), options).await
}

/// Validate a config without applying it.
pub async fn validate_config(config: Value, options: Option<CoreIpcOptions>) -> AppResult<Value> {
    if !config.is_object() {
        return Err(crate::errors::AppError::invalid_argument(
            "config must be a JSON object",
        ));
    }
    run_command("config.validate", json!({ "config": config }), options).await
}

/// Dry-run config apply — returns impact analysis without applying changes.
///
/// Sends `config.plan_apply` to the kernel, which returns a structured
/// breakdown of which sections can be hot-reloaded and which require
/// a kernel restart.
pub async fn plan_apply_config(
    config: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiConfigPlanApplyResult> {
    if !config.is_object() {
        return Err(crate::errors::AppError::invalid_argument(
            "config must be a JSON object",
        ));
    }
    let value = run_command("config.plan_apply", json!({ "config": config }), options).await?;
    Ok(parse_plan_apply_result(&value))
}

/// Set the global routing mode at runtime (hot-switch, no restart).
pub async fn set_mode(
    mode: String,
    outbound: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let mut params = Map::new();
    params.insert("mode".to_string(), json!(mode));
    if let Some(outbound) = outbound {
        params.insert("outbound".to_string(), json!(outbound));
    }
    run_command("mode.set", Value::Object(params), options).await
}

/// DNS lookup diagnostic.
pub async fn dns_lookup(hostname: String, options: Option<CoreIpcOptions>) -> AppResult<Value> {
    let hostname = normalize_non_empty(hostname, "hostname")?;
    run_command(
        "diagnostics.dns_lookup",
        json!({ "hostname": hostname }),
        options,
    )
    .await
}

/// Route trace diagnostic.
pub async fn trace_route(
    target: String,
    port: u16,
    protocol: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let target = normalize_non_empty(target, "target")?;
    let mut params = Map::new();
    params.insert("target".to_string(), json!(target));
    params.insert("port".to_string(), json!(port));
    if let Some(protocol) = protocol {
        params.insert("protocol".to_string(), json!(protocol));
    }
    run_command("diagnostics.trace_route", Value::Object(params), options).await
}

/// Enable TUN virtual network interface.
pub async fn enable_tun(
    tun_name: Option<String>,
    tun_addr: String,
    tun_tag: String,
    tun_mtu: u16,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiFeatureStatus> {
    let mut params = Map::new();
    if let Some(name) = tun_name {
        params.insert("name".to_string(), json!(name));
    }
    params.insert("addr".to_string(), json!(tun_addr));
    params.insert("tag".to_string(), json!(tun_tag));
    params.insert("mtu".to_string(), json!(tun_mtu));

    let value = run_command("tun.start", Value::Object(params), options).await?;
    Ok(parse_feature_runtime_status("tun", &value, None))
}

/// Disable TUN virtual network interface.
pub async fn disable_tun(options: Option<CoreIpcOptions>) -> AppResult<GuiFeatureStatus> {
    let value = run_command("tun.stop", json!({}), options).await?;
    Ok(parse_feature_runtime_status("tun", &value, None))
}

// ── Internal helpers ────────────────────────────────────────────────

/// Send a command and unwrap the response. Shared with queries.rs.
pub(crate) async fn run_command(
    method: &str,
    params: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let call = protocol::command(method.to_string(), Some(params), options).await?;
    unwrap_call_result(call.response, call.error)
}
