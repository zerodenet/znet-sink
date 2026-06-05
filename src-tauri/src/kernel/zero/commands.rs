//! Zero kernel command methods.
//!
//! Each function sends an IPC command and parses the response into a
//! GUI model type. Stateless — receives `CoreIpcOptions` directly.

use serde_json::{json, Map, Value};

use crate::errors::AppResult;
use crate::kernel::protocol;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::{
    GuiConnectionCloseResult, GuiFeatureStatus, GuiPolicySelectionResult, GuiTargetProbeResult,
};

use super::parsing::{
    normalize_non_empty, parse_connection_close, parse_feature_runtime_status,
    parse_policy_selection, parse_target_probe, unwrap_call_result,
};

/// Switch the selected outbound in a policy group.
pub async fn select_policy(
    policy_tag: String,
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiPolicySelectionResult> {
    let policy_tag = normalize_non_empty(policy_tag, "policyTag")?;
    let target_tag = normalize_non_empty(target_tag, "targetTag")?;
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
