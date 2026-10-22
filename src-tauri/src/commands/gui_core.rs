#[cfg(feature = "tool-dns")]
use crate::models::gui_core::GuiFakeIpClearInput;
#[cfg(feature = "tool-dns")]
use crate::models::gui_core::GuiFakeIpClearResult;
#[cfg(feature = "tool-node-probe")]
use crate::services::probe;
use serde::Deserialize;
use std::time::{Duration, Instant};
#[cfg(feature = "tool-node-probe")]
use tauri::Emitter;
use tauri::{AppHandle, Manager, State};

use crate::client_core::{ClientCoreSnapshot, ClientScope, NodeScreenSnapshot};
#[cfg(feature = "tool-node-probe")]
use crate::client_core::{ProbeJobId, ProbeJobKind, ProbeJobSnapshot, StartProbeRequest};
use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{self, build_traffic_snapshot, TrafficSample, ZeroAdapter};
use crate::models::app_config::AppDnsConfig;
use crate::models::core_process::CoreProcessState;
use crate::models::dns_config::ClientDnsConfig;
use crate::models::gui_core::{
    ConfigProxyNode, GuiConnection, GuiConnectionCloseResult, GuiConnectionList,
    GuiConnectionListOptions, GuiCoreHealth, GuiCoreOverview, GuiFeatureStatus, GuiPolicyGroup,
    GuiPolicySelectionResult, GuiTrafficSnapshot, GuiTrafficStats, GuiZeroCapabilities,
};
use crate::models::zero_runtime::GuiTunStatus;
use crate::services::common;
use crate::services::{core_config, core_process, diagnostic_storage};
use crate::state::app_state::AppState;

mod diagnostic_export;
mod dns_apply;
mod dns_transaction;

const CORE_READY_WAIT_TIMEOUT: Duration = Duration::from_secs(8);
const CORE_READY_WAIT_INTERVAL: Duration = Duration::from_millis(100);

/// Return the revisioned authoritative client scope. This is the recovery
/// point for future ordered node/probe updates and is intentionally a thin
/// Tauri adapter over the Rust Client Core.
#[tauri::command]
pub fn gui_client_core_snapshot(state: State<'_, AppState>) -> ClientCoreSnapshot {
    state.client_core_snapshot()
}

#[tauri::command]
pub async fn gui_node_screen_snapshot(
    state: State<'_, AppState>,
    reason: Option<String>,
) -> AppResult<NodeScreenSnapshot> {
    crate::services::node_screen::snapshot(state.inner(), reason.as_deref()).await
}

#[cfg(feature = "tool-node-probe")]
#[tauri::command]
pub(crate) fn gui_probe_runtime_snapshot(
    state: State<'_, AppState>,
) -> probe::runtime::ProbeRuntimeSnapshot {
    state.probe_runtime().snapshot()
}

#[cfg(feature = "tool-node-probe")]
#[tauri::command]
pub fn gui_probe_job_get(
    state: State<'_, AppState>,
    job_id: ProbeJobId,
) -> AppResult<ProbeJobSnapshot> {
    state
        .get_client_probe_job(job_id)
        .ok_or_else(|| AppError::not_found("probe_job", job_id.0.to_string()))
}

#[cfg(feature = "tool-node-probe")]
#[tauri::command]
pub fn gui_probe_job_list(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> Vec<ProbeJobSnapshot> {
    state.list_client_probe_jobs(profile_id)
}

#[cfg(feature = "tool-node-probe")]
#[tauri::command]
pub fn gui_probe_job_cancel(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    job_id: ProbeJobId,
) -> AppResult<ProbeJobSnapshot> {
    let job = state
        .cancel_client_probe(job_id)
        .ok_or_else(|| AppError::not_found("probe_job", job_id.0.to_string()))?;
    let _ = app_handle.emit(probe::PROBE_JOB_UPDATED_EVENT, job.clone());
    Ok(job)
}

#[cfg(feature = "tool-node-probe")]
#[tauri::command]
pub async fn gui_probe_job_start(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: StartProbeRequest,
    expected_scope: Option<ClientScope>,
) -> AppResult<ProbeJobSnapshot> {
    let _operation = state.proxy_config_operation().lock().await;
    state.ensure_client_scope(expected_scope.as_ref())?;
    if request.kind == ProbeJobKind::ScheduledPolicyObservation {
        return Err(AppError::invalid_argument(
            "scheduled policy observations are recorded from kernel events and cannot be started manually",
        ));
    }
    let request = probe::normalize_start_request(state.inner(), request)?;
    let outcome = state
        .start_client_probe_scoped(request, expected_scope.as_ref())
        .map_err(AppError::client_core)?;
    if outcome.created {
        if let Err(error) = state.probe_runtime().checkpoint(state.capabilities()) {
            let _ = state.cancel_client_probe(outcome.job.id);
            return Err(error);
        }
        probe::spawn_probe_timeout(app_handle.clone(), &outcome.job);
        tauri::async_runtime::spawn(probe::run_probe_job(app_handle, outcome.job.clone()));
    }
    Ok(outcome.job)
}

fn default_opts(state: &AppState) -> crate::models::core::CoreIpcOptions {
    core_config::ipc_options_from_app_config(
        &common::lock(state.app_config(), "app_config")
            .map(|c| c.core.clone())
            .unwrap_or_default(),
    )
}

#[tauri::command]
pub async fn gui_core_overview(state: State<'_, AppState>) -> AppResult<GuiCoreOverview> {
    let process = core_process::refresh_status(state.inner())?;
    let adapter = ZeroAdapter::new();
    let opts = default_opts(state.inner());
    let result = adapter
        .core_overview(process.state == CoreProcessState::Running, opts)
        .await;

    let health = result.health;
    let config = result.config;
    let capabilities = result.capabilities;
    let stats = result.stats;
    let policy_groups = result.policy_groups;
    let available = result.available;
    let last_error = result.last_error;

    Ok(GuiCoreOverview {
        process,
        available,
        health,
        config,
        stats,
        policy_groups,
        capabilities,
        last_error,
    })
}

#[tauri::command]
pub async fn gui_core_health(state: State<'_, AppState>) -> AppResult<GuiCoreHealth> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().health(opts).await
}

#[tauri::command]
pub async fn gui_zero_capabilities(state: State<'_, AppState>) -> AppResult<GuiZeroCapabilities> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().capabilities(opts).await
}

#[tauri::command]
pub async fn gui_traffic_stats(state: State<'_, AppState>) -> AppResult<GuiTrafficStats> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().traffic_stats(opts).await
}

#[tauri::command]
pub async fn gui_traffic_snapshot(state: State<'_, AppState>) -> AppResult<GuiTrafficSnapshot> {
    let adapter = ZeroAdapter::new();
    let opts = default_opts(state.inner());
    let totals = adapter.traffic_stats(opts).await?;
    let sampled_at_unix_ms = common::now_unix_ms();

    let previous = state
        .traffic_sample()
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let snapshot = build_traffic_snapshot(totals.clone(), previous.as_ref(), sampled_at_unix_ms);
    if let Ok(mut sample) = state.traffic_sample().lock() {
        *sample = Some(TrafficSample {
            stats: totals,
            sampled_at_unix_ms,
        });
    }

    Ok(snapshot)
}

#[tauri::command]
pub async fn gui_policy_groups(state: State<'_, AppState>) -> AppResult<Vec<GuiPolicyGroup>> {
    let adapter = ZeroAdapter::new();
    let opts = default_opts(state.inner());

    // Always extract the protocol map from config so we can enrich
    // kernel runtime data with protocol types the kernel doesn't return.
    let active_content = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .and_then(|p| p.content.clone());
    let kind_map = active_content
        .as_ref()
        .map(zero::config::outbound_kind_map)
        .unwrap_or_default();

    match adapter.policy_groups(opts).await {
        Ok(mut groups) if !groups.is_empty() => {
            // Layer config-sourced protocol types onto kernel runtime data.
            for group in &mut groups {
                for member in &mut group.outbounds {
                    if member.kind.is_none() || member.kind.as_deref() == Some("unknown") {
                        member.kind = kind_map.get(&member.tag).cloned();
                    }
                }
            }
            Ok(groups)
        }
        Ok(_) | Err(_) => {
            // Fallback: extract from static config
            let config_content = active_content.unwrap_or(serde_json::json!({}));
            adapter.policy_groups_from_config(&config_content)
        }
    }
}

#[tauri::command]
pub async fn gui_select_policy(
    state: State<'_, AppState>,
    policy_tag: String,
    target_tag: String,
    expected_scope: Option<ClientScope>,
) -> AppResult<GuiPolicySelectionResult> {
    let _operation = state.proxy_config_operation().lock().await;
    state.ensure_client_scope(expected_scope.as_ref())?;
    let opts = default_opts(state.inner());
    let result = ZeroAdapter::new()
        .select_policy(policy_tag.clone(), target_tag.clone(), opts)
        .await?;
    if result.accepted {
        let selected = result.selected.as_deref().unwrap_or(&target_tag);
        crate::services::policy_selection::record_active_configuration_selection(
            state.inner(),
            &policy_tag,
            selected,
        )?;
    }
    Ok(result)
}

#[tauri::command]
pub async fn gui_observation_snapshot(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    crate::services::flow_observation::connect(state.inner())
        .await?
        .snapshot()
        .await
}

#[tauri::command]
pub async fn gui_connections(
    state: State<'_, AppState>,
    options: Option<GuiConnectionListOptions>,
) -> AppResult<GuiConnectionList> {
    crate::services::flow_observation::connect(state.inner())
        .await?
        .active(options)
        .await
}

#[tauri::command]
pub async fn gui_connection_detail(
    state: State<'_, AppState>,
    flow_id: String,
) -> AppResult<GuiConnection> {
    crate::services::flow_observation::connect(state.inner())
        .await?
        .detail(&flow_id)
        .await
}

#[tauri::command]
pub async fn gui_close_connection(
    state: State<'_, AppState>,
    flow_id: String,
) -> AppResult<GuiConnectionCloseResult> {
    let _ = (state, flow_id);
    Err(AppError {
        code: "unsupported",
        message: "旧连接关闭入口不具备原实例与目标绑定；请重新核对并选择连接关闭".into(),
        details: None,
    })
}

#[tauri::command]
pub async fn gui_dns_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().dns_status(opts).await
}

#[tauri::command]
pub async fn gui_tun_status(state: State<'_, AppState>) -> AppResult<GuiTunStatus> {
    let opts = default_opts(state.inner());
    zero::runtime::tun_status(Some(opts)).await
}

#[tauri::command]
pub async fn gui_tun_enable(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiTunStatus> {
    let _operation = state.proxy_config_operation().lock().await;
    crate::services::native_operation::execute_async(
        state.inner(),
        "system.tun",
        "enable",
        Duration::from_secs(60),
        async {
            ensure_core_ready(app_handle, state.clone()).await?;
            let tun = { common::lock(state.app_config(), "app_config")?.tun.clone() };
            let opts = default_opts(state.inner());
            zero::runtime::enable_tun_params(
                crate::configuration::preferences::tun_params(state.inner(), tun)?,
                Some(opts),
            )
            .await
        },
    )
    .await
}

#[tauri::command]
pub async fn gui_tun_recover(state: State<'_, AppState>) -> AppResult<GuiTunStatus> {
    let _operation = state.proxy_config_operation().lock().await;
    crate::services::native_operation::execute_async(
        state.inner(),
        "system.tun",
        "recover",
        Duration::from_secs(60),
        zero::runtime::recover_tun(Some(default_opts(state.inner()))),
    )
    .await
}

#[tauri::command]
pub async fn gui_tun_disable(state: State<'_, AppState>) -> AppResult<GuiTunStatus> {
    let _operation = state.proxy_config_operation().lock().await;
    let opts = default_opts(state.inner());
    crate::services::native_operation::execute_async(
        state.inner(),
        "system.tun",
        "disable",
        Duration::from_secs(60),
        zero::runtime::disable_tun(Some(opts)),
    )
    .await
}

#[tauri::command]
pub async fn gui_stack_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().stack_status(opts).await
}

#[tauri::command]
pub async fn gui_rule_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().rule_status(opts).await
}

/// Return the node list directly from the active proxy config file.
/// Does NOT require the core to be running — this is static config data.
#[tauri::command]
pub fn gui_proxy_nodes(state: State<'_, AppState>) -> AppResult<Vec<ConfigProxyNode>> {
    let active = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .cloned();

    let Some(active) = active else {
        return Ok(Vec::new());
    };

    let Some(content) = &active.content else {
        return Ok(Vec::new());
    };

    let adapter = ZeroAdapter::new();
    adapter.proxy_nodes_from_config(content)
}

/// Return policy groups directly from the active proxy config file.
/// Does NOT require the core to be running — this is static config data.
/// The frontend uses this as the skeleton for the node page sidebar,
/// with kernel runtime state (selected, latency, alive) layered on top.
#[tauri::command]
pub fn gui_config_policy_groups(state: State<'_, AppState>) -> AppResult<Vec<GuiPolicyGroup>> {
    let active = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .cloned();

    let Some(active) = active else {
        return Ok(Vec::new());
    };

    let Some(content) = &active.content else {
        return Ok(Vec::new());
    };

    let adapter = ZeroAdapter::new();
    adapter.policy_groups_from_config(content)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDnsSettingsInput {
    pub enabled: bool,
    #[serde(default)]
    pub config: Option<ClientDnsConfig>,
    #[serde(default)]
    pub dns_hijack: bool,
}

fn validate_dns_settings(input: &GuiDnsSettingsInput) -> AppResult<()> {
    if input.enabled {
        let config = input.config.as_ref().ok_or_else(|| {
            AppError::invalid_argument("global DNS requires a configuration when enabled")
        })?;
        config.validate_client_shape()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn gui_apply_dns_config(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: GuiDnsSettingsInput,
) -> AppResult<serde_json::Value> {
    validate_dns_settings(&input)?;
    let _operation = state.proxy_config_operation().lock().await;
    dns_apply::apply(app_handle, state.clone(), input).await
}

/// Validate a config without applying it.
#[tauri::command]
pub async fn gui_validate_config(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    let effective =
        crate::services::rule_overlay::compose_effective_config(state.inner(), &config)?;
    ZeroAdapter::new().validate_config(effective, opts).await
}

#[tauri::command]
pub async fn gui_validate_dns_config(
    state: State<'_, AppState>,
    input: GuiDnsSettingsInput,
) -> AppResult<serde_json::Value> {
    validate_dns_settings(&input)?;
    let active_content = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone());
    let Some(content) = active_content else {
        return Ok(serde_json::json!({ "valid": true, "errors": [] }));
    };
    let dns = AppDnsConfig {
        enabled: input.enabled,
        config: input.config,
        dns_hijack: input.enabled && input.dns_hijack,
    };
    let effective = crate::services::rule_overlay::compose_effective_config_with_dns(
        state.inner(),
        &content,
        &dns,
    )?;
    ZeroAdapter::new()
        .validate_config(effective, default_opts(state.inner()))
        .await
}

/// Inspect the exact base and client-composed configuration without applying it.
/// This is intentionally local: it explains what ZNet-Sink will send to Zero
/// and never mutates the active profile or runtime.
#[tauri::command]
pub fn gui_inspect_dns_effective_config(
    state: State<'_, AppState>,
    input: GuiDnsSettingsInput,
) -> AppResult<serde_json::Value> {
    validate_dns_settings(&input)?;
    let active = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .map(|profile| (profile.name.clone(), profile.content.clone()));
    let Some((profile_name, Some(base))) = active else {
        return Ok(serde_json::json!({
            "activeProfileName": null,
            "baseConfig": null,
            "effectiveConfig": null,
            "sources": [],
            "reason": "no_active_proxy_profile"
        }));
    };
    let dns = AppDnsConfig {
        enabled: input.enabled,
        config: input.config,
        dns_hijack: input.enabled && input.dns_hijack,
    };
    let effective = crate::services::rule_overlay::compose_effective_config_with_dns(
        state.inner(),
        &base,
        &dns,
    )?;
    let common_rules = crate::services::rule_overlay::status(state.inner())?;
    let mut sources = vec![serde_json::json!({
        "id": "global_dns",
        "label": "全局 DNS",
        "paths": ["runtime.dns"],
        "enabled": dns.enabled
    })];
    sources.push(serde_json::json!({
        "id": "tun_dns_hijack",
        "label": "TUN DNS 劫持启动参数",
        "paths": ["client.tun.dns_hijack"],
        "enabled": dns.dns_hijack
    }));
    sources.push(serde_json::json!({
        "id": "common_rules",
        "label": "公共规则覆盖",
        "paths": ["route.rule_sets", "route.rules"],
        "enabled": common_rules.effective,
        "count": common_rules.injected_count
    }));
    Ok(serde_json::json!({
        "activeProfileName": profile_name,
        "baseConfig": base,
        "effectiveConfig": effective,
        "sources": sources,
        "reason": null
    }))
}

/// Dry-run config apply — returns impact analysis without applying changes.
///
/// Sends `config.plan_apply` to the kernel, which returns a structured
/// breakdown of which sections can be hot-reloaded and which require
/// a kernel restart.
/// Set the global routing mode at runtime (hot-switch, no kernel restart).
#[tauri::command]
pub async fn gui_set_mode(
    state: State<'_, AppState>,
    mode: String,
    outbound: Option<String>,
) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    crate::services::native_operation::execute_async(
        state.inner(),
        "runtime.mode",
        &mode,
        Duration::from_secs(30),
        ZeroAdapter::new().set_mode(mode.clone(), outbound, opts),
    )
    .await
}

/// DNS lookup diagnostic.
#[cfg(feature = "tool-dns")]
#[tauri::command]
pub async fn gui_dns_lookup(
    state: State<'_, AppState>,
    hostname: String,
) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().dns_lookup(hostname, opts).await
}

#[cfg(feature = "tool-dns")]
#[tauri::command]
pub async fn gui_dns_cache(
    state: State<'_, AppState>,
    domain: Option<String>,
    limit: Option<usize>,
) -> AppResult<serde_json::Value> {
    ZeroAdapter::new()
        .dns_cache(domain, limit, default_opts(state.inner()))
        .await
}

#[cfg(feature = "tool-dns")]
#[tauri::command]
pub async fn gui_fakeip_lookup(
    state: State<'_, AppState>,
    domain: Option<String>,
    ip: Option<String>,
) -> AppResult<serde_json::Value> {
    ZeroAdapter::new()
        .fakeip_lookup(domain, ip, default_opts(state.inner()))
        .await
}

/// Clear all Fake-IP mappings or one mapping selected by domain/address.
#[cfg(feature = "tool-dns")]
#[tauri::command]
pub async fn gui_clear_fake_ip(
    state: State<'_, AppState>,
    input: Option<GuiFakeIpClearInput>,
) -> AppResult<GuiFakeIpClearResult> {
    let input = input.unwrap_or_default();
    let opts = default_opts(state.inner());
    ZeroAdapter::new()
        .clear_fake_ip(input.domain, input.ip, opts)
        .await
}

/// Route trace diagnostic.
#[cfg(feature = "tool-route")]
#[tauri::command]
pub async fn gui_trace_route(
    state: State<'_, AppState>,
    target: String,
    port: Option<u16>,
    protocol: Option<String>,
    inbound_tag: Option<String>,
) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new()
        .trace_route(target, port.unwrap_or(80), protocol, inbound_tag, opts)
        .await
}

/// Query recently completed connections.
#[tauri::command]
pub async fn gui_recent_connections(
    state: State<'_, AppState>,
    options: Option<GuiConnectionListOptions>,
) -> AppResult<GuiConnectionList> {
    crate::services::flow_observation::connect(state.inner())
        .await?
        .recent(options)
        .await
}

/// Query event sink delivery status.
#[tauri::command]
pub async fn gui_sinks(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().sinks(opts).await
}

/// Query diagnostics overview.
#[tauri::command]
pub async fn gui_diagnostics(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().diagnostics(opts).await
}

async fn ensure_core_ready(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let adapter = ZeroAdapter::new();
    let opts = default_opts(state.inner());
    if adapter.readiness_health(opts).await.is_ok() {
        return Ok(());
    }

    let app_handle_start = app_handle.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle_start.state::<AppState>();
        core_process::start(app_handle_start.clone(), state)
    })
    .await
    .map_err(|error| AppError::internal(format!("core start thread panicked: {error}")))??;

    if status.state != CoreProcessState::Running {
        return Err(AppError::internal(
            "core process did not enter running state",
        ));
    }

    wait_for_core_ready(app_handle.state::<AppState>().inner()).await
}

async fn wait_for_core_ready(state: &AppState) -> AppResult<()> {
    let started = Instant::now();
    let mut last_error = None;
    let adapter = ZeroAdapter::new();

    while started.elapsed() < CORE_READY_WAIT_TIMEOUT {
        let opts = default_opts(state);
        match adapter.readiness_health(opts).await {
            Ok(health) if health.healthy => return Ok(()),
            Ok(_) => {
                return Err(AppError::internal(
                    "core readiness check reported unhealthy",
                ))
            }
            Err(error) => {
                last_error = Some(error);
                let _ = tauri::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(CORE_READY_WAIT_INTERVAL);
                })
                .await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::internal("core readiness check timed out")))
}

/// Detect the host machine's current public IP and geo information.
/// This GUI-side request does not call the kernel; it follows the host's
/// current system network and proxy configuration.
#[tauri::command]
pub async fn gui_network_probe(
    state: State<'_, AppState>,
) -> AppResult<crate::services::network_probe::NetworkProbeResult> {
    let probe_urls = {
        let config = common::lock(state.app_config(), "app_config")?;
        config.core.network_probe_urls.clone()
    };
    let capabilities = state.capabilities().clone();

    tauri::async_runtime::spawn_blocking(move || {
        crate::services::network_probe::probe_local_network(&capabilities, &probe_urls)
    })
    .await
    .map_err(|e| AppError::internal(format!("network probe task failed: {}", e)))?
}

/// Get the GUI / core log file paths and data directory.
#[tauri::command]
pub fn gui_log_paths() -> AppResult<GuiLogPaths> {
    let data_dir = crate::services::data_dir()?;
    let logs_dir = data_dir.join("logs");
    let log_file = logs_dir.join("gui.log.jsonl");
    let core_log_file = crate::services::core_config::managed_core_log_path()?;

    Ok(GuiLogPaths {
        data_dir: data_dir.to_string_lossy().to_string(),
        logs_dir: logs_dir.to_string_lossy().to_string(),
        log_file: log_file.to_string_lossy().to_string(),
        core_log_file: core_log_file.to_string_lossy().to_string(),
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiLogPaths {
    pub data_dir: String,
    pub logs_dir: String,
    pub log_file: String,
    pub core_log_file: String,
}

#[tauri::command]
pub async fn gui_debug_storage_summary() -> AppResult<diagnostic_storage::DebugStorageSummary> {
    tauri::async_runtime::spawn_blocking(diagnostic_storage::summary)
        .await
        .map_err(|error| AppError::internal(format!("debug storage worker failed: {error}")))?
}

#[tauri::command]
pub async fn gui_clear_debug_storage(
    app_handle: AppHandle,
) -> AppResult<diagnostic_storage::DebugStorageCleanupResult> {
    let state = app_handle.state::<AppState>();
    let cleanup_app = app_handle.clone();
    crate::services::native_operation::execute_async(
        state.inner(),
        "diagnostic.clear",
        "managed-storage",
        Duration::from_secs(60),
        async move {
            tauri::async_runtime::spawn_blocking(move || {
                let state = cleanup_app.state::<AppState>();
                diagnostic_storage::clear(state.inner())
            })
            .await
            .map_err(|error| AppError::internal(format!("debug cleanup worker failed: {error}")))?
        },
    )
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDiagnosticExport {
    pub directory: String,
    pub files: Vec<String>,
    pub created_at_unix_ms: u64,
}

/// Deliver only the immutable public projection that the user previewed.
/// Arbitrary stored messages, fields, payloads and files are never copied.
#[tauri::command]
pub async fn gui_export_diagnostics(
    app_handle: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    plan_id: String,
) -> AppResult<GuiDiagnosticExport> {
    diagnostic_export::admit(window.label(), crate::mobile_host::snapshot().mobile)?;
    crate::services::native_operation::execute_async(
        state.inner(),
        "diagnostic.export",
        "sanitized-bundle",
        Duration::from_secs(60),
        async move {
            tauri::async_runtime::spawn_blocking(move || {
                let state = app_handle.state::<AppState>();
                diagnostic_export::export(state.inner(), &plan_id)
            })
            .await
            .map_err(|_| AppError::internal("诊断导出结果未确认，请检查原目标；原计划不会重放"))?
        },
    )
    .await
}

/// Release only an unsubmitted diagnostic preview; never undo a delivery.
#[tauri::command]
pub fn gui_discard_diagnostics(
    window: tauri::Window,
    state: State<'_, AppState>,
    plan_id: String,
) -> AppResult<()> {
    diagnostic_export::admit(window.label(), crate::mobile_host::snapshot().mobile)?;
    crate::services::diagnostics::discard(state.inner(), &plan_id)
}
