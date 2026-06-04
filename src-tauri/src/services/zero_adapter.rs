use serde_json::{Map, Value, json};

use crate::errors::{AppError, AppResult};
use crate::models::{
    core::CoreIpcOptions,
    gui_core::{
        GuiCapabilityEndpoint, GuiConnection, GuiConnectionCloseResult, GuiConnectionList,
        GuiConnectionListOptions, GuiCoreHealth, GuiCoreOverview, GuiFeatureStatus, GuiPolicyGroup,
        GuiPolicyMember, GuiPolicySelectionResult, GuiTrafficRates, GuiTrafficSnapshot,
        GuiTrafficStats, GuiZeroCapabilities,
    },
};
use crate::services::{common, common::lock, control_plane, core_config, core_process};
use crate::state::app_state::{AppState, TrafficSample};

const DEFAULT_CONNECTION_LIMIT: u32 = 100;
const MAX_CONNECTION_LIMIT: u32 = 500;
const MIN_TRAFFIC_SAMPLE_INTERVAL_MS: u64 = 500;

pub async fn core_overview(state: &AppState) -> AppResult<GuiCoreOverview> {
    let process = core_process::refresh_status(state)?;
    let capabilities = zero_capabilities(state).await;
    let mut last_error = capabilities
        .as_ref()
        .err()
        .map(|error| error.message.clone());
    let capabilities = capabilities.unwrap_or_else(|error| GuiZeroCapabilities {
        available: false,
        error: Some(error.message),
        ..GuiZeroCapabilities::default()
    });

    let health = match core_readiness_health(state).await {
        Ok(health) => Some(health),
        Err(error) => {
            last_error.get_or_insert(error.message);
            None
        }
    };
    let stats = match traffic_stats(state).await {
        Ok(stats) => stats,
        Err(error) => {
            last_error.get_or_insert(error.message);
            GuiTrafficStats::default()
        }
    };
    let policy_groups = match policy_groups(state).await {
        Ok(groups) => groups,
        Err(error) => {
            last_error.get_or_insert(error.message);
            Vec::new()
        }
    };

    Ok(GuiCoreOverview {
        process,
        available: capabilities.available || health.as_ref().is_some_and(|health| health.healthy),
        health,
        stats,
        policy_groups,
        capabilities,
        last_error,
    })
}

/// Query the core's detailed health info (version, uptime, etc.).
///
/// Prefer [`core_readiness_health`] for liveness checks — it pings first.
pub async fn core_health(state: &AppState) -> AppResult<GuiCoreHealth> {
    let value = query_result_with_timeout(state, json!({"type":"health"}), 1_500).await?;
    Ok(parse_health(&value))
}

/// Fast liveness check — pings first, then optionally enriches with the
/// detailed Health query for version and uptime metadata.
pub async fn core_readiness_health(state: &AppState) -> AppResult<GuiCoreHealth> {
    let ping = control_plane::ping(default_options(state)?).await?;
    unwrap_call_result(ping.response, ping.error)?;

    match query_result_with_timeout(state, json!({"type":"health"}), 2_000).await {
        Ok(value) => Ok(parse_health(&value)),
        Err(_) => Ok(GuiCoreHealth {
            healthy: true,
            engine_version: None,
            started_at_unix_ms: None,
        }),
    }
}

pub async fn traffic_stats(state: &AppState) -> AppResult<GuiTrafficStats> {
    let value = query_result(state, json!({"type":"stats"})).await?;
    Ok(parse_stats(&value))
}

pub async fn traffic_snapshot(state: &AppState) -> AppResult<GuiTrafficSnapshot> {
    let totals = traffic_stats(state).await?;
    let sampled_at_unix_ms = common::now_unix_ms();
    Ok(build_traffic_snapshot(state, totals, sampled_at_unix_ms)?)
}

pub async fn zero_capabilities(state: &AppState) -> AppResult<GuiZeroCapabilities> {
    let value = query_result(state, json!({"type":"capabilities"})).await?;
    Ok(parse_capabilities(&value, None))
}

pub async fn capability_feature_keys(state: &AppState) -> AppResult<Vec<String>> {
    Ok(zero_capabilities(state).await?.features)
}

pub async fn policy_groups(state: &AppState) -> AppResult<Vec<GuiPolicyGroup>> {
    crate::services::logs::znet_log(
        Some(state),
        crate::models::logs::LogLevel::Info,
        "policies query: sending request to core".to_string(),
    );
    match query_result(state, json!({"type":"policies"})).await {
        Ok(value) => {
            let groups = parse_policy_groups(&value);
            crate::services::logs::znet_log(
                Some(state),
                crate::models::logs::LogLevel::Info,
                format!(
                    "policies query: got {} groups, raw={}",
                    groups.len(),
                    serde_json::to_string(&value).unwrap_or_else(|_| "<err>".to_string())
                ),
            );
            Ok(groups)
        }
        Err(error) => {
            crate::services::logs::znet_log(
                Some(state),
                crate::models::logs::LogLevel::Warn,
                format!("policies query failed: {}", error.message),
            );
            Err(error)
        }
    }
}

pub async fn select_policy(
    state: &AppState,
    policy_tag: String,
    target_tag: String,
) -> AppResult<GuiPolicySelectionResult> {
    let policy_tag = normalize_non_empty(policy_tag, "policyTag")?;
    let target_tag = normalize_non_empty(target_tag, "targetTag")?;
    let value = command_result(
        state,
        "policies.select",
        json!({
            "policy_tag": policy_tag,
            "target_tag": target_tag,
        }),
    )
    .await?;

    Ok(parse_policy_selection(&value, policy_tag, target_tag))
}

pub async fn connections(
    state: &AppState,
    options: Option<GuiConnectionListOptions>,
) -> AppResult<GuiConnectionList> {
    let options = options.unwrap_or(GuiConnectionListOptions {
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
        state,
        json!({
            "type": "active_flows",
            "limit": limit,
            "filter": Value::Object(filter),
        }),
    )
    .await?;

    Ok(parse_connection_list(&value, limit))
}

pub async fn connection_detail(state: &AppState, flow_id: String) -> AppResult<GuiConnection> {
    let flow_id = normalize_non_empty(flow_id, "flowId")?;
    let value = query_result(state, json!({ "type": "flow", "flow_id": flow_id })).await?;
    parse_connection(&value).ok_or_else(|| AppError::invalid_argument("core returned invalid flow"))
}

pub async fn close_connection(
    state: &AppState,
    flow_id: String,
) -> AppResult<GuiConnectionCloseResult> {
    let flow_id = normalize_non_empty(flow_id, "flowId")?;
    let value = command_result(state, "flows.close", json!({ "flow_id": flow_id })).await?;
    Ok(GuiConnectionCloseResult {
        flow_id: string_at(&value, &["flow_id", "flowId"]).unwrap_or_default(),
        closed: bool_at(&value, &["closed"]).unwrap_or(true),
        message: string_at(&value, &["message"]),
    })
}

pub async fn dns_status(state: &AppState) -> AppResult<GuiFeatureStatus> {
    feature_status(state, "dns", &["dns", "dns-status", "dns-snapshot"]).await
}

pub async fn tun_status(state: &AppState) -> AppResult<GuiFeatureStatus> {
    let fallback = feature_status(state, "tun", &["tun", "tun-status", "tun-snapshot"]).await;

    for request in [json!({"type":"tun_status"})] {
        if let Ok(value) = query_result(state, request).await {
            return Ok(parse_feature_runtime_status("tun", &value, fallback.as_ref().ok()));
        }
    }

    if let Ok(value) = command_result(state, "tun.status", json!({})).await {
        return Ok(parse_feature_runtime_status("tun", &value, fallback.as_ref().ok()));
    }

    fallback
}

pub async fn enable_tun(state: &AppState) -> AppResult<GuiFeatureStatus> {
    core_readiness_health(state).await?;
    let tun = { lock(state.app_config(), "app_config")?.tun.clone() };
    let mut params = Map::new();
    if let Some(name) = normalize_optional(tun.name) {
        params.insert("name".to_string(), json!(name));
    }
    params.insert("addr".to_string(), json!(tun.addr));
    params.insert("tag".to_string(), json!(tun.tag));
    params.insert("mtu".to_string(), json!(tun.mtu));

    let value = command_result(state, "tun.start", Value::Object(params)).await?;
    Ok(parse_feature_runtime_status("tun", &value, None))
}

pub async fn disable_tun(state: &AppState) -> AppResult<GuiFeatureStatus> {
    core_readiness_health(state).await?;
    let value = command_result(state, "tun.stop", json!({})).await?;
    Ok(parse_feature_runtime_status("tun", &value, None))
}

pub async fn stack_status(state: &AppState) -> AppResult<GuiFeatureStatus> {
    feature_status(state, "stack", &["system-stack", "stack", "stack-status"]).await
}

pub async fn rule_status(state: &AppState) -> AppResult<GuiFeatureStatus> {
    feature_status(
        state,
        "rules",
        &["rules", "rule-status", "rule-snapshot", "routing"],
    )
    .await
}

async fn feature_status(
    state: &AppState,
    key: &'static str,
    candidates: &[&str],
) -> AppResult<GuiFeatureStatus> {
    let capabilities = zero_capabilities(state).await?;
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

fn parse_feature_runtime_status(
    key: &'static str,
    value: &Value,
    fallback: Option<&GuiFeatureStatus>,
) -> GuiFeatureStatus {
    let status = nested_value(value, &["result"])
        .or_else(|| nested_value(value, &["status"]))
        .or_else(|| nested_value(value, &[key]))
        .unwrap_or(value);
    let enabled = bool_at(status, &["running", "enabled", "active"])
        .or_else(|| string_at(status, &["state", "status"]).map(|state| {
            matches!(
                state.to_ascii_lowercase().as_str(),
                "running" | "started" | "active" | "enabled"
            )
        }))
        .unwrap_or(false);
    let state = string_at(status, &["state", "status"])
        .unwrap_or_else(|| if enabled { "running" } else { "stopped" }.to_string());
    let reason = string_at(status, &["reason", "message", "error"])
        .or_else(|| fallback.and_then(|fallback| fallback.reason.clone()));

    GuiFeatureStatus {
        key: key.to_string(),
        supported: fallback.map(|fallback| fallback.supported).unwrap_or(true),
        enabled,
        state,
        reason,
    }
}

async fn query_result(state: &AppState, request: Value) -> AppResult<Value> {
    let call = control_plane::query(request, default_options(state)?).await?;
    unwrap_call_result(call.response, call.error)
}

async fn command_result(state: &AppState, method: &str, params: Value) -> AppResult<Value> {
    let call =
        control_plane::command(method.to_string(), Some(params), default_options(state)?).await?;
    unwrap_call_result(call.response, call.error)
}

fn default_options(state: &AppState) -> AppResult<Option<CoreIpcOptions>> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    Ok(Some(core_config::ipc_options_from_app_config(&config)))
}

async fn query_result_with_timeout(
    state: &AppState,
    request: Value,
    timeout_ms: u64,
) -> AppResult<Value> {
    let mut opts =
        core_config::ipc_options_from_app_config(&lock(state.app_config(), "app_config")?.core);
    opts.timeout_ms = Some(timeout_ms);
    let call = control_plane::query(request, Some(opts)).await?;
    unwrap_call_result(call.response, call.error)
}

fn unwrap_call_result(response: Option<Value>, error: Option<AppError>) -> AppResult<Value> {
    if let Some(error) = error {
        return Err(error);
    }

    let response = response.ok_or_else(|| AppError::internal("core returned no response"))?;
    unwrap_core_envelope(response)
}

fn unwrap_core_envelope(response: Value) -> AppResult<Value> {
    let Some(object) = response.as_object() else {
        return Ok(response);
    };

    if let Some(false) = object.get("ok").and_then(Value::as_bool) {
        return Err(AppError::core_response(Value::Object(object.clone())));
    }
    if object.contains_key("ok") {
        return Ok(object.get("result").cloned().unwrap_or(Value::Null));
    }

    Ok(Value::Object(object.clone()))
}

fn parse_health(value: &Value) -> GuiCoreHealth {
    GuiCoreHealth {
        healthy: bool_at(value, &["healthy"]).unwrap_or(true),
        engine_version: normalize_version(string_at(
            value,
            &["engine_version", "engineVersion", "version"],
        )),
        started_at_unix_ms: u64_at(
            value,
            &["started_at_unix_ms", "startedAtUnixMs", "started_at"],
        ),
    }
}

pub(crate) fn parse_stats(value: &Value) -> GuiTrafficStats {
    let stats = nested_value(value, &["stats"]).unwrap_or(value);
    GuiTrafficStats {
        active_sessions: u64_at(stats, &["active_sessions", "activeSessions"]).unwrap_or(0),
        total_started: u64_at(stats, &["total_started", "totalStarted"]).unwrap_or(0),
        completed_sessions: u64_at(stats, &["completed_sessions", "completedSessions"])
            .unwrap_or(0),
        failed_sessions: u64_at(stats, &["failed_sessions", "failedSessions"]).unwrap_or(0),
        blocked_sessions: u64_at(stats, &["blocked_sessions", "blockedSessions"]).unwrap_or(0),
        direct_sessions: u64_at(stats, &["direct_sessions", "directSessions"]).unwrap_or(0),
        chained_sessions: u64_at(stats, &["chained_sessions", "chainedSessions"]).unwrap_or(0),
        bytes_up: u64_at(stats, &["bytes_up", "bytesUp", "upload", "tx"]).unwrap_or(0),
        bytes_down: u64_at(stats, &["bytes_down", "bytesDown", "download", "rx"]).unwrap_or(0),
    }
}

pub(crate) fn build_traffic_snapshot(
    state: &AppState,
    totals: GuiTrafficStats,
    sampled_at_unix_ms: u64,
) -> AppResult<GuiTrafficSnapshot> {
    let mut sample = lock(state.traffic_sample(), "traffic_sample")?;
    let previous = sample.clone();
    let mut interval_ms = previous
        .as_ref()
        .map(|previous| sampled_at_unix_ms.saturating_sub(previous.sampled_at_unix_ms))
        .filter(|interval| *interval > 0);
    let mut stable = true;
    let mut reason = None;
    let mut rates = GuiTrafficRates::default();

    match previous {
        Some(previous) if interval_ms.unwrap_or(0) >= MIN_TRAFFIC_SAMPLE_INTERVAL_MS => {
            let interval = interval_ms.expect("interval is checked above");
            rates = calculate_rates(&previous.stats, &totals, interval);
        }
        Some(_) => {
            stable = false;
            reason = Some("sample interval is too short for a stable rate".to_string());
        }
        None => {
            stable = false;
            interval_ms = None;
            reason = Some("first sample has no previous traffic baseline".to_string());
        }
    }

    *sample = Some(TrafficSample {
        stats: totals.clone(),
        sampled_at_unix_ms,
    });

    Ok(GuiTrafficSnapshot {
        totals,
        rates,
        sampled_at_unix_ms,
        interval_ms,
        source: "zero-stats".to_string(),
        stable,
        reason,
    })
}

pub(crate) fn calculate_rates(
    previous: &GuiTrafficStats,
    current: &GuiTrafficStats,
    interval_ms: u64,
) -> GuiTrafficRates {
    if interval_ms == 0 {
        return GuiTrafficRates::default();
    }

    GuiTrafficRates {
        upload_bps: bytes_delta_per_second(previous.bytes_up, current.bytes_up, interval_ms),
        download_bps: bytes_delta_per_second(previous.bytes_down, current.bytes_down, interval_ms),
    }
}

pub(crate) fn bytes_delta_per_second(previous: u64, current: u64, interval_ms: u64) -> u64 {
    if current < previous || interval_ms == 0 {
        return 0;
    }

    let delta = u128::from(current - previous);
    let rate = delta.saturating_mul(1000) / u128::from(interval_ms);
    rate.min(u128::from(u64::MAX)) as u64
}

fn parse_capabilities(value: &Value, error: Option<String>) -> GuiZeroCapabilities {
    GuiZeroCapabilities {
        available: error.is_none(),
        api_version: string_at(value, &["api_version", "apiVersion"]),
        schema_version: string_at(value, &["schema_version", "schemaVersion"]),
        features: string_array_at(value, &["features"]),
        permissions: string_array_at(value, &["permissions"]),
        adapters: endpoint_array_at(value, "adapters"),
        sinks: endpoint_array_at(value, "sinks"),
        error,
    }
}

fn parse_policy_groups(value: &Value) -> Vec<GuiPolicyGroup> {
    values_from_container(
        value,
        &[
            "policies",
            "policy_groups",
            "policyGroups",
            "groups",
            "outbounds",
            "items",
        ],
    )
    .into_iter()
    .filter_map(parse_policy_group)
    .collect()
}

fn parse_policy_group(value: Value) -> Option<GuiPolicyGroup> {
    let tag = string_at(&value, &["policy_tag", "policyTag", "tag", "name", "id"])?;
    let selected = string_at(&value, &["selected", "current", "now", "target"]);
    let members = parse_policy_members(&value, selected.as_deref());

    Some(GuiPolicyGroup {
        tag,
        kind: string_at(&value, &["policy_kind", "policyKind", "kind", "type"])
            .unwrap_or_else(|| "unknown".to_string()),
        selected,
        members,
        available: bool_at(&value, &["available", "healthy"]).unwrap_or(true),
        reason: string_at(&value, &["reason", "error", "message"]),
    })
}

fn parse_policy_members(value: &Value, selected: Option<&str>) -> Vec<GuiPolicyMember> {
    values_from_container(
        value,
        &["members", "targets", "children", "proxies", "outbounds", "items"],
    )
    .into_iter()
    .filter_map(|member| parse_policy_member(member, selected))
    .collect()
}

fn parse_policy_member(value: Value, selected: Option<&str>) -> Option<GuiPolicyMember> {
    let tag = match &value {
        Value::String(tag) => tag.clone(),
        other => string_at(other, &["tag", "name", "id", "target"])?,
    };
    let source = if value.is_object() { Some(value) } else { None };
    let kind = source
        .as_ref()
        .and_then(|value| string_at(value, &["kind", "type", "protocol"]));
    let alive = source
        .as_ref()
        .and_then(|value| bool_at(value, &["alive", "healthy", "available"]));
    let delay_ms = source
        .as_ref()
        .and_then(|value| u64_at(value, &["delay_ms", "delayMs", "latency", "latency_ms"]));

    Some(GuiPolicyMember {
        selected: selected.is_some_and(|selected| selected == tag),
        tag,
        kind,
        alive,
        delay_ms,
    })
}

fn parse_policy_selection(
    value: &Value,
    policy_tag: String,
    target_tag: String,
) -> GuiPolicySelectionResult {
    let result = nested_value(value, &["result"]).unwrap_or(value);
    GuiPolicySelectionResult {
        policy_tag: string_at(result, &["policy_tag", "policyTag"]).unwrap_or(policy_tag),
        target_tag,
        selected: string_at(result, &["selected", "target_tag", "targetTag"]),
        accepted: bool_at(value, &["accepted"]).unwrap_or(true),
        message: string_at(result, &["message"]),
    }
}

fn parse_connection_list(value: &Value, limit: u32) -> GuiConnectionList {
    let items = values_from_container(value, &["flows", "connections", "items", "data", "active"])
        .into_iter()
        .filter_map(|value| parse_connection(&value))
        .collect::<Vec<_>>();

    GuiConnectionList {
        total: u64_at(value, &["total", "count"]),
        items,
        limit,
    }
}

pub(crate) fn parse_connection(value: &Value) -> Option<GuiConnection> {
    let flow_id = string_at(
        value,
        &["flow_id", "flowId", "id", "connection_id", "connectionId"],
    )?;
    let target = nested_value(value, &["target"]);
    let host = target
        .and_then(|target| string_at(target, &["host", "address"]))
        .or_else(|| string_at(value, &["host", "destination", "dest", "remote", "address"]));
    let port = target
        .and_then(|target| u64_at(target, &["port"]))
        .or_else(|| {
            u64_at(
                value,
                &["port", "dest_port", "destPort", "remote_port", "remotePort"],
            )
        });
    let destination = match (host, port) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host,
        (None, Some(port)) => port.to_string(),
        (None, None) => "-".to_string(),
    };
    let traffic = nested_value(value, &["traffic"]).unwrap_or(value);
    let timing = nested_value(value, &["timing"]).unwrap_or(value);

    Some(GuiConnection {
        flow_id,
        network: string_at(value, &["network", "protocol", "type"])
            .unwrap_or_else(|| "tcp".to_string()),
        source: string_at(value, &["source", "client", "local"]),
        destination,
        inbound_tag: nested_value(value, &["inbound"])
            .and_then(|inbound| string_at(inbound, &["tag", "protocol"]))
            .or_else(|| string_at(value, &["inbound_tag", "inboundTag"])),
        outbound_tag: nested_value(value, &["outbound"])
            .and_then(|outbound| string_at(outbound, &["tag", "protocol"]))
            .or_else(|| string_at(value, &["outbound_tag", "outboundTag"])),
        policy_tag: nested_value(value, &["policy"])
            .and_then(|policy| string_at(policy, &["tag", "policy_tag", "policyTag"]))
            .or_else(|| string_at(value, &["policy_tag", "policyTag"])),
        route_mode: nested_value(value, &["route"])
            .and_then(|route| string_at(route, &["mode"]))
            .or_else(|| string_at(value, &["route_mode", "routeMode"])),
        outcome: string_at(value, &["outcome", "status"]),
        bytes_up: u64_at(traffic, &["bytes_up", "bytesUp", "tx"]).unwrap_or(0),
        bytes_down: u64_at(traffic, &["bytes_down", "bytesDown", "rx"]).unwrap_or(0),
        throughput_up_bps: u64_at(value, &["throughput_up_bps", "throughputUpBps"]),
        throughput_down_bps: u64_at(value, &["throughput_down_bps", "throughputDownBps"]),
        started_at_unix_ms: u64_at(
            timing,
            &["started_at_unix_ms", "startedAtUnixMs", "started_at"],
        ),
        updated_at_unix_ms: u64_at(
            value,
            &["snapshot_at_unix_ms", "updatedAtUnixMs", "updated_at"],
        ),
        duration_ms: u64_at(timing, &["duration_ms", "durationMs"]),
    })
}

fn values_from_container(value: &Value, keys: &[&str]) -> Vec<Value> {
    if let Some(array) = value.as_array() {
        return array.clone();
    }

    for key in keys {
        if let Some(candidate) = value.get(*key) {
            if let Some(array) = candidate.as_array() {
                return array.clone();
            }
            if let Some(object) = candidate.as_object() {
                return object.values().cloned().collect();
            }
        }
    }

    value
        .as_object()
        .map(|object| object.values().cloned().collect())
        .unwrap_or_default()
}

fn endpoint_array_at(value: &Value, key: &str) -> Vec<GuiCapabilityEndpoint> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GuiCapabilityEndpoint {
                        kind: string_at(item, &["kind", "type", "name"])?,
                        enabled: bool_at(item, &["enabled"]).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn nested_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| value.get(*key))
}

pub(crate) fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
    })
}

fn string_array_at(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn u64_at(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
    })
}

fn bool_at(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value.as_bool().or_else(|| {
                value
                    .as_str()
                    .and_then(|value| match value.to_ascii_lowercase().as_str() {
                        "true" | "yes" | "1" => Some(true),
                        "false" | "no" | "0" => Some(false),
                        _ => None,
                    })
            })
        })
    })
}

/// Strip leading 'v' from version strings so all comparisons are prefix-free.
/// Both kernel CLI (`zero --version`) and GitHub tags use v-prefixed names,
/// but internal comparisons and the updater expect bare semver.
pub(crate) fn normalize_version(version: Option<String>) -> Option<String> {
    version.map(|v| v.strip_prefix('v').unwrap_or(&v).to_string())
}

fn normalize_non_empty(value: String, field: &'static str) -> AppResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::invalid_argument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}
