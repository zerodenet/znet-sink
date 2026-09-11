//! Zero kernel command methods.
//!
//! Each function sends an IPC command and parses the response into a
//! GUI model type. Stateless — receives `CoreIpcOptions` directly.

#[cfg(feature = "tool-node-probe")]
use crate::models::gui_core::GuiTargetProbeResult;
use serde_json::{json, Map, Value};
use std::time::Duration;

use crate::errors::AppResult;
use crate::kernel::protocol;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::{
    GuiConnectionCloseResult, GuiFeatureStatus, GuiPolicySelectionResult,
};

#[cfg(any(
    feature = "tool-dns",
    feature = "tool-route",
    feature = "tool-node-probe"
))]
use super::parsing::normalize_optional;
#[cfg(feature = "tool-node-probe")]
use super::parsing::parse_target_probe;
use super::parsing::{
    normalize_non_empty, parse_connection_close, parse_feature_runtime_status,
    parse_policy_selection, unwrap_call_result,
};

/// Outbound diagnostics can legitimately queue behind other probes in the
/// kernel and take tens of seconds. The process watchdog remains responsible
/// for detecting a genuinely unresponsive IPC channel.
#[cfg(feature = "tool-node-probe")]
const PROBE_IPC_TIMEOUT_MS: u64 = crate::config::MAX_IPC_TIMEOUT_MS;

/// Creating/removing the device and routes can exceed the ordinary two-second
/// IPC budget. Keep this at the adapter command boundary: manual toggles,
/// startup/restart restoration and legacy adapter callers all pass here.
const TUN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

fn command_response_timeout(method: &str) -> Option<Duration> {
    match method {
        "tun.start" | "tun.stop" | "tun.recover" => Some(TUN_RESPONSE_TIMEOUT),
        _ => None,
    }
}

#[cfg(test)]
#[path = "commands/timeout_tests.rs"]
mod timeout_tests;

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
#[cfg(feature = "tool-node-probe")]
pub async fn probe_policy(policy_tag: String, options: Option<CoreIpcOptions>) -> AppResult<Value> {
    probe_policy_with_operation_id(policy_tag, None, options).await
}

#[cfg(feature = "tool-node-probe")]
pub async fn probe_policy_with_operation_id(
    policy_tag: String,
    operation_id: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let policy_tag = normalize_non_empty(policy_tag, "policyTag")?;
    let mut params = json!({ "policy_tag": policy_tag });
    if let Some(operation_id) = operation_id {
        params["operation_id"] = Value::String(operation_id);
    }
    run_command("policies.probe", params, options).await
}

/// Normalize legacy policy-probe acknowledgement fields at the Zero boundary.
/// Older kernels omitted these flags, which remains a compatible acceptance.
#[cfg(feature = "tool-node-probe")]
pub fn policy_probe_command_accepted(response: &Value) -> bool {
    response.get("accepted").and_then(Value::as_bool) != Some(false)
        && response
            .get("result")
            .and_then(|result| {
                result
                    .get("probeTriggered")
                    .or_else(|| result.get("probe_triggered"))
            })
            .and_then(Value::as_bool)
            != Some(false)
}

#[cfg(feature = "tool-node-probe")]
pub fn policy_probe_operation_id(response: &Value) -> Option<&str> {
    response
        .get("result")
        .and_then(|result| {
            result
                .get("operationId")
                .or_else(|| result.get("operation_id"))
        })
        .and_then(Value::as_str)
}

/// Probe a single target for reachability and latency.
#[cfg(feature = "tool-node-probe")]
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
#[cfg(feature = "tool-node-probe")]
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
///
/// Flow rows are event-driven, so a flow may naturally complete between the
/// user's click and the `flows.close` command reaching Zero. For the GUI,
/// "already absent" satisfies the requested end state and is therefore
/// normalized to an idempotent success. Transport failures and every other
/// core error remain real failures.
pub async fn close_connection(
    flow_id: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiConnectionCloseResult> {
    let flow_id = normalize_non_empty(flow_id, "flowId")?;
    match run_command(
        "flows.close",
        json!({ "flow_id": flow_id.clone() }),
        options,
    )
    .await
    {
        Ok(value) => Ok(parse_connection_close(&value, flow_id)),
        Err(error) if is_flow_already_completed_error(&error) => Ok(GuiConnectionCloseResult {
            flow_id,
            closed: true,
            message: Some("flow already completed".to_string()),
        }),
        Err(error) => Err(error),
    }
}

pub(crate) fn is_flow_already_completed_error(error: &crate::errors::AppError) -> bool {
    if error.code == "not_found" {
        return true;
    }
    if error.code != "core_error" {
        return false;
    }
    let message = error.message.to_ascii_lowercase();
    message.contains("flow `")
        && message.contains("not found")
        && message.contains("already completed")
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

#[cfg(feature = "tool-node-probe")]
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
    let call = protocol::command_with_response_timeout(
        method.to_string(),
        Some(params),
        options,
        command_response_timeout(method),
    )
    .await?;
    unwrap_call_result(call.response, call.error)
}

pub(crate) fn ensure_config_apply_accepted(response: &Value) -> AppResult<()> {
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
    #[cfg(feature = "tool-dns")]
    use super::dns::fake_ip_clear_params;
    #[cfg(feature = "tool-route")]
    use super::route::trace_route_params;
    use super::{ensure_config_apply_accepted, is_flow_already_completed_error};
    #[cfg(feature = "tool-node-probe")]
    use super::{policy_probe_command_accepted, probe_ipc_options, PROBE_IPC_TIMEOUT_MS};
    use crate::errors::AppError;
    #[cfg(feature = "tool-dns")]
    use crate::kernel::zero::parsing::parse_fake_ip_clear;
    #[cfg(feature = "tool-node-probe")]
    use crate::models::core::CoreIpcOptions;
    use serde_json::json;

    #[cfg(feature = "tool-route")]
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

    #[cfg(feature = "tool-dns")]
    #[test]
    fn fake_ip_clear_params_support_full_and_targeted_management() {
        assert_eq!(
            fake_ip_clear_params(None, None).expect("clear all params"),
            json!({})
        );
        assert_eq!(
            fake_ip_clear_params(Some(" example.com ".to_string()), None).expect("domain params"),
            json!({ "domain": "example.com" })
        );
        assert!(fake_ip_clear_params(
            Some("example.com".to_string()),
            Some("198.18.0.1".to_string())
        )
        .is_err());
    }

    #[cfg(feature = "tool-dns")]
    #[test]
    fn fake_ip_clear_response_is_normalized_for_the_gui() {
        let result = parse_fake_ip_clear(&json!({
            "accepted": true,
            "result": {
                "core_instance_id": "core-1",
                "config_revision": 7,
                "enabled": true,
                "scope": "domain",
                "domain": "example.com",
                "removed_mappings": 1,
                "removed_addresses": 2,
                "live_mappings": 3,
                "retired_addresses": 4
            }
        }));

        assert_eq!(result.core_instance_id.as_deref(), Some("core-1"));
        assert_eq!(result.config_revision, Some(7));
        assert_eq!(result.scope, "domain");
        assert_eq!(result.domain.as_deref(), Some("example.com"));
        assert_eq!(result.removed_mappings, 1);
        assert_eq!(result.removed_addresses, 2);
        assert_eq!(result.live_mappings, 3);
        assert_eq!(result.retired_addresses, 4);
    }

    #[cfg(feature = "tool-node-probe")]
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

    #[cfg(feature = "tool-node-probe")]
    #[test]
    fn policy_probe_ack_compatibility_is_normalized_at_zero_boundary() {
        assert!(!policy_probe_command_accepted(
            &json!({ "accepted": false })
        ));
        assert!(!policy_probe_command_accepted(&json!({
            "result": { "probeTriggered": false }
        })));
        assert!(!policy_probe_command_accepted(&json!({
            "result": { "probe_triggered": false }
        })));
        assert!(policy_probe_command_accepted(&json!({ "accepted": true })));
        assert!(policy_probe_command_accepted(&json!({})));
    }

    #[test]
    fn flow_close_accepts_stable_and_legacy_already_completed_errors() {
        let stable_not_found = AppError::core_response(json!({
            "error": {
                "code": "not_found",
                "message": "flow `22397` not found or already completed"
            }
        }));
        assert!(is_flow_already_completed_error(&stable_not_found));

        let already_completed = AppError::core_response(json!({
            "error": {
                "message": "flow `22397` not found or already completed"
            }
        }));
        assert!(is_flow_already_completed_error(&already_completed));

        let unrelated_not_found = AppError::core_response(json!({
            "error": {
                "message": "policy group not found"
            }
        }));
        assert!(!is_flow_already_completed_error(&unrelated_not_found));

        let transport_failure = AppError::internal("flow `22397` not found or already completed");
        assert!(!is_flow_already_completed_error(&transport_failure));
    }
}

#[cfg(feature = "tool-dns")]
mod dns;
#[cfg(feature = "tool-dns")]
pub use dns::{clear_fake_ip, dns_cache, dns_lookup, fakeip_lookup};
#[cfg(feature = "tool-route")]
mod route;
#[cfg(feature = "tool-route")]
pub use route::trace_route;
