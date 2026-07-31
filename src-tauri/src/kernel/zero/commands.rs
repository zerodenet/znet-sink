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
    normalize_non_empty, normalize_optional, parse_connection_close, parse_feature_runtime_status,
    parse_policy_selection, parse_target_probe, unwrap_call_result,
};

/// Outbound diagnostics can legitimately queue behind other probes in the
/// kernel and take tens of seconds. The process watchdog remains responsible
/// for detecting a genuinely unresponsive IPC channel.
const PROBE_IPC_TIMEOUT_MS: u64 = crate::config::MAX_IPC_TIMEOUT_MS;

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
    let options = probe_ipc_options(options);
    let value = run_command(
        "diagnostics.probe_target",
        json!({ "target_tag": target_tag }),
        options,
    )
    .await?;

    Ok(parse_target_probe(&value, target_tag))
}

/// Probe a single outbound through the kernel's full proxy stack.
pub async fn probe_outbound(
    target_tag: String,
    url: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiTargetProbeResult> {
    let target_tag = normalize_non_empty(target_tag, "targetTag")?;
    let options = probe_ipc_options(options);
    let mut params = Map::new();
    params.insert("target_tag".to_string(), json!(target_tag));
    if let Some(url) = normalize_optional(url) {
        params.insert("url".to_string(), json!(url));
    }
    let value = run_command("diagnostics.probe_outbound", Value::Object(params), options).await?;
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
    let response = run_command("config.apply", json!({ "config": config }), options).await?;
    ensure_config_apply_accepted(&response)?;
    Ok(response)
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
    inbound_tag: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let target = normalize_non_empty(target, "target")?;
    let params = trace_route_params(target, port, protocol, inbound_tag);
    run_command("diagnostics.trace_route", params, options).await
}

fn trace_route_params(
    target: String,
    port: u16,
    protocol: Option<String>,
    inbound_tag: Option<String>,
) -> Value {
    let mut params = Map::new();
    params.insert("target".to_string(), json!(target));
    params.insert("port".to_string(), json!(port));
    if let Some(protocol) = normalize_optional(protocol) {
        params.insert("protocol".to_string(), json!(protocol));
    }
    if let Some(inbound_tag) = normalize_optional(inbound_tag) {
        params.insert("inbound_tag".to_string(), json!(inbound_tag));
    }
    Value::Object(params)
}

fn probe_ipc_options(options: Option<CoreIpcOptions>) -> Option<CoreIpcOptions> {
    let mut options = options.unwrap_or_default();
    options.timeout_ms = Some(PROBE_IPC_TIMEOUT_MS);
    Some(options)
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

fn ensure_config_apply_accepted(response: &Value) -> AppResult<()> {
    if response.get("accepted").and_then(Value::as_bool) == Some(false) {
        return Err(crate::errors::AppError::core_response(response.clone()));
    }
    if response
        .get("result")
        .and_then(|result| result.get("applied"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Err(crate::errors::AppError::core_response(response.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_config_apply_accepted, probe_ipc_options, trace_route_params, PROBE_IPC_TIMEOUT_MS,
    };
    use crate::models::core::CoreIpcOptions;
    use serde_json::json;

    #[test]
    fn trace_route_preserves_optional_inbound_tag() {
        assert_eq!(
            trace_route_params(
                "example.com".to_string(),
                443,
                Some("tcp".to_string()),
                Some("mixed-in".to_string()),
            ),
            json!({
                "target": "example.com",
                "port": 443,
                "protocol": "tcp",
                "inbound_tag": "mixed-in"
            })
        );
    }

    #[test]
    fn outbound_probe_uses_a_bounded_long_response_timeout() {
        let options = probe_ipc_options(Some(CoreIpcOptions {
            socket: Some("test-pipe".to_string()),
            timeout_ms: Some(2_000),
        }))
        .expect("probe options");

        assert_eq!(options.socket.as_deref(), Some("test-pipe"));
        assert_eq!(options.timeout_ms, Some(PROBE_IPC_TIMEOUT_MS));
    }

    #[test]
    fn config_apply_rejects_unaccepted_or_unapplied_response() {
        assert!(ensure_config_apply_accepted(&json!({
            "accepted": false,
            "result": { "applied": false }
        }))
        .is_err());
        assert!(ensure_config_apply_accepted(&json!({
            "accepted": true,
            "result": { "applied": false }
        }))
        .is_err());
        assert!(ensure_config_apply_accepted(&json!({
            "accepted": true,
            "result": { "applied": true }
        }))
        .is_ok());
    }
}
