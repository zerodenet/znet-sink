use std::time::{Duration, Instant};
use std::{
    fs,
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::client_core::{
    ClientCoreSnapshot, NodeScreenSnapshot, ProbeJobId, ProbeJobKind, ProbeJobSnapshot,
    StartProbeRequest,
};
use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{self, build_traffic_snapshot, TrafficSample, ZeroAdapter};
use crate::models::core_process::CoreProcessState;
use crate::models::gui_core::{
    ConfigProxyNode, GuiConnection, GuiConnectionCloseResult, GuiConnectionList,
    GuiConnectionListOptions, GuiCoreHealth, GuiCoreOverview, GuiFeatureStatus, GuiPolicyGroup,
    GuiPolicySelectionResult, GuiTrafficSnapshot, GuiTrafficStats, GuiZeroCapabilities,
};
use crate::models::zero_runtime::GuiTunStatus;
use crate::services::common;
use crate::services::{
    core_config, core_process, diagnostic_storage, interaction_mode, probe, proxy_config,
};
use crate::state::app_state::AppState;

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

#[tauri::command]
pub fn gui_probe_job_get(
    state: State<'_, AppState>,
    job_id: ProbeJobId,
) -> AppResult<ProbeJobSnapshot> {
    state
        .get_client_probe_job(job_id)
        .ok_or_else(|| AppError::not_found("probe_job", job_id.0.to_string()))
}

#[tauri::command]
pub fn gui_probe_job_list(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> Vec<ProbeJobSnapshot> {
    state.list_client_probe_jobs(profile_id)
}

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

#[tauri::command]
pub fn gui_probe_job_start(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: StartProbeRequest,
) -> AppResult<ProbeJobSnapshot> {
    if request.kind == ProbeJobKind::ScheduledPolicyObservation {
        return Err(AppError::invalid_argument(
            "scheduled policy observations are recorded from kernel events and cannot be started manually",
        ));
    }
    let request = probe::normalize_start_request(state.inner(), request)?;
    let outcome = state
        .start_client_probe(request)
        .map_err(AppError::client_core)?;
    if outcome.created {
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
) -> AppResult<GuiPolicySelectionResult> {
    let _operation = state.proxy_config_operation().lock().await;
    let opts = default_opts(state.inner());
    let result = ZeroAdapter::new()
        .select_policy(policy_tag.clone(), target_tag.clone(), opts)
        .await?;
    if result.accepted {
        let selected = result.selected.as_deref().unwrap_or(&target_tag);
        if crate::services::policy_selection::record_active_subscription_selection(
            state.inner(),
            &policy_tag,
            selected,
        )? {
            // Keep the exported launch config in sync as well, so both a
            // managed restart and an external Zero restart restore the choice.
            core_config::export_active(state.clone())?;
        }
    }
    Ok(result)
}

/// Probe a single outbound through the kernel proxy stack.
///
/// Fire-and-forget like `gui_probe_policy`: spawns the IPC probe in background,
/// returns immediately. Results arrive via `diagnostics.probe_outbound` response
/// logged to the event stream, or the frontend can poll via policy status.
#[tauri::command]
pub async fn gui_probe_target(
    state: State<'_, AppState>,
    target_tag: String,
) -> AppResult<serde_json::Value> {
    let adapter = ZeroAdapter::new();
    let opts = default_opts(state.inner());
    // Quick health check first — fail fast if kernel is offline
    if adapter.readiness_health(opts).await.is_err() {
        return Ok(serde_json::json!({"accepted": false, "reason": "kernel offline"}));
    }
    let opts = default_opts(state.inner());
    tauri::async_runtime::spawn(async move {
        let _ = adapter.probe_outbound(target_tag, None, opts).await;
    });
    Ok(serde_json::json!({"accepted": true}))
}

#[tauri::command]
pub async fn gui_connections(
    state: State<'_, AppState>,
    options: Option<GuiConnectionListOptions>,
) -> AppResult<GuiConnectionList> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().connections(options, opts).await
}

#[tauri::command]
pub async fn gui_connection_detail(
    state: State<'_, AppState>,
    flow_id: String,
) -> AppResult<GuiConnection> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().connection_detail(flow_id, opts).await
}

#[tauri::command]
pub async fn gui_close_connection(
    state: State<'_, AppState>,
    flow_id: String,
) -> AppResult<GuiConnectionCloseResult> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().close_connection(flow_id, opts).await
}

#[tauri::command]
pub async fn gui_dns_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    interaction_mode::require_pro_mode(state.inner(), "dns")?;
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
    ensure_core_ready(app_handle, state.clone()).await?;
    let tun = { common::lock(state.app_config(), "app_config")?.tun.clone() };
    let opts = default_opts(state.inner());
    zero::runtime::enable_tun(tun, Some(opts)).await
}

#[tauri::command]
pub async fn gui_tun_disable(state: State<'_, AppState>) -> AppResult<GuiTunStatus> {
    let _operation = state.proxy_config_operation().lock().await;
    let opts = default_opts(state.inner());
    zero::runtime::disable_tun(Some(opts)).await
}

#[tauri::command]
pub async fn gui_stack_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    interaction_mode::require_pro_mode(state.inner(), "stack")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().stack_status(opts).await
}

#[tauri::command]
pub async fn gui_rule_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    interaction_mode::require_pro_mode(state.inner(), "rules")?;
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

/// Apply a config to the running kernel without restart (hot-reload).
#[tauri::command]
pub async fn gui_apply_config(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "apply_config")?;
    let _operation = state.proxy_config_operation().lock().await;
    let previous_content = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone())
        .ok_or_else(|| {
            AppError::invalid_argument(
                "an active proxy config with parsed content is required before applying changes",
            )
        })?;
    let opts = default_opts(state.inner());
    let effective =
        crate::services::rule_overlay::compose_effective_config(state.inner(), &config)?;
    let result = ZeroAdapter::new().apply_config(effective, opts).await?;
    // The kernel accepted the config — mirror it into the active profile so
    // that config-derived views (proxy nodes, policy groups) and the next
    // core-process start reflect the live configuration.
    if let Err(error) = proxy_config::update_active_content(state.inner(), config) {
        let previous_effective = crate::services::rule_overlay::compose_effective_config(
            state.inner(),
            &previous_content,
        )?;
        let _ = ZeroAdapter::new()
            .apply_config(previous_effective, default_opts(state.inner()))
            .await;
        return Err(error);
    }
    if let Err(error) = proxy_config::retarget_managed_system_proxy(state.inner()) {
        let _ = proxy_config::update_active_content(state.inner(), previous_content.clone());
        let previous_effective = crate::services::rule_overlay::compose_effective_config(
            state.inner(),
            &previous_content,
        )?;
        let _ = ZeroAdapter::new()
            .apply_config(previous_effective, default_opts(state.inner()))
            .await;
        let _ = proxy_config::retarget_managed_system_proxy(state.inner());
        return Err(error);
    }
    Ok(result)
}

/// Validate a config without applying it.
#[tauri::command]
pub async fn gui_validate_config(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "validate_config")?;
    let opts = default_opts(state.inner());
    let effective =
        crate::services::rule_overlay::compose_effective_config(state.inner(), &config)?;
    ZeroAdapter::new().validate_config(effective, opts).await
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
    ZeroAdapter::new().set_mode(mode, outbound, opts).await
}

/// Trigger a url_test probe on a policy group.
///
/// Waits only for the kernel's command acknowledgement. Probe results arrive
/// later via `policy.probeCompleted` events.
#[tauri::command]
pub async fn gui_probe_policy(
    state: State<'_, AppState>,
    policy_tag: String,
) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().probe_policy(policy_tag, opts).await
}

/// DNS lookup diagnostic.
#[tauri::command]
pub async fn gui_dns_lookup(
    state: State<'_, AppState>,
    hostname: String,
) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "dns_lookup")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().dns_lookup(hostname, opts).await
}

/// Route trace diagnostic.
#[tauri::command]
pub async fn gui_trace_route(
    state: State<'_, AppState>,
    target: String,
    port: Option<u16>,
    protocol: Option<String>,
    inbound_tag: Option<String>,
) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "trace_route")?;
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
    interaction_mode::require_pro_mode(state.inner(), "recent_connections")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().recent_connections(options, opts).await
}

/// Query event sink delivery status.
#[tauri::command]
pub async fn gui_sinks(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "sinks")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().sinks(opts).await
}

/// Query diagnostics overview.
#[tauri::command]
pub async fn gui_diagnostics(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "diagnostics")?;
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

    tauri::async_runtime::spawn_blocking(move || {
        crate::services::network_probe::probe_local_network(&probe_urls)
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
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        diagnostic_storage::clear(state.inner())
    })
    .await
    .map_err(|error| AppError::internal(format!("debug cleanup worker failed: {error}")))?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiDiagnosticExport {
    pub directory: String,
    pub files: Vec<String>,
    pub created_at_unix_ms: u64,
}

/// Export a local, support-ready diagnostic directory without proxy config
/// contents, subscription URLs, credentials, or other user secrets.
#[tauri::command]
pub async fn gui_export_diagnostics() -> AppResult<GuiDiagnosticExport> {
    tauri::async_runtime::spawn_blocking(export_diagnostics)
        .await
        .map_err(|error| AppError::internal(format!("diagnostic export worker failed: {error}")))?
}

fn export_diagnostics() -> AppResult<GuiDiagnosticExport> {
    diagnostic_storage::with_storage_lock(export_diagnostics_locked)
}

fn export_diagnostics_locked() -> AppResult<GuiDiagnosticExport> {
    let created_at_unix_ms = common::now_unix_ms();
    let data_dir = crate::services::data_dir()?;
    let export_dir = data_dir
        .join("diagnostics")
        .join(format!("znet-sink-diagnostics-{created_at_unix_ms}"));
    fs::create_dir_all(&export_dir).map_err(|error| {
        AppError::internal(format!("create diagnostic export directory: {error}"))
    })?;

    let candidates = diagnostic_storage::diagnostic_log_paths()?;
    let mut files = Vec::new();
    for source in candidates {
        if !source.is_file() {
            continue;
        }
        let Some(file_name) = source.file_name() else {
            continue;
        };
        let destination = export_dir.join(file_name);
        copy_diagnostic_file(&source, &destination)?;
        files.push(file_name.to_string_lossy().to_string());
    }

    let manifest_name = "manifest.json";
    let manifest = serde_json::json!({
        "schema": "znet.diagnostics.v1",
        "createdAtUnixMs": created_at_unix_ms,
        "appVersion": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "files": files,
        "privacy": {
            "proxyConfigIncluded": false,
            "subscriptionUrlsIncluded": false,
            "credentialsIncluded": false,
            "structuredLogsSanitized": true
        }
    });
    write_pretty_json(&export_dir.join(manifest_name), &manifest)?;
    files.push(manifest_name.to_string());

    crate::services::file_logger::emit(
        "info",
        "diagnostics",
        "diagnostic export created",
        Some(serde_json::json!({
            "directory": export_dir.to_string_lossy(),
            "file_count": files.len()
        })),
    );

    Ok(GuiDiagnosticExport {
        directory: export_dir.to_string_lossy().to_string(),
        files,
        created_at_unix_ms,
    })
}

fn copy_diagnostic_file(source: &Path, destination: &Path) -> AppResult<()> {
    match source.file_name().and_then(|name| name.to_str()) {
        Some("debug.log.jsonl") => {
            write_sanitized_jsonl(source, destination, sanitize_debug_record)
        }
        Some(name) if name.ends_with(".jsonl") => {
            write_sanitized_jsonl(source, destination, sanitize_structured_record)
        }
        _ => fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| AppError::internal(format!("copy diagnostic file: {error}"))),
    }
}

fn write_sanitized_jsonl(
    source: &Path,
    destination: &Path,
    sanitize: fn(&mut serde_json::Value),
) -> AppResult<()> {
    let input = fs::File::open(source)
        .map_err(|error| AppError::internal(format!("open diagnostic log: {error}")))?;
    let output = fs::File::create(destination)
        .map_err(|error| AppError::internal(format!("create diagnostic log: {error}")))?;
    let mut writer = BufWriter::new(output);

    for line in BufReader::new(input).lines() {
        let line =
            line.map_err(|error| AppError::internal(format!("read diagnostic log: {error}")))?;
        if line.trim().is_empty() {
            continue;
        }
        let mut value = serde_json::from_str::<serde_json::Value>(&line).unwrap_or_else(|_| {
            serde_json::json!({
                "redacted": true,
                "reason": "unparseable diagnostic record"
            })
        });
        sanitize(&mut value);
        serde_json::to_writer(&mut writer, &value)
            .map_err(|error| AppError::internal(format!("serialize diagnostic log: {error}")))?;
        writer
            .write_all(b"\n")
            .map_err(|error| AppError::internal(format!("write diagnostic log: {error}")))?;
    }
    writer
        .flush()
        .map_err(|error| AppError::internal(format!("flush diagnostic log: {error}")))
}

fn sanitize_debug_record(value: &mut serde_json::Value) {
    if let Some(payload) = value.get_mut("payload") {
        *payload = summarize_debug_payload(payload);
    }
    sanitize_structured_record(value);
}

fn summarize_debug_payload(payload: &serde_json::Value) -> serde_json::Value {
    let Some(object) = payload.as_object() else {
        return serde_json::json!({ "redacted": true });
    };

    let mut summary = serde_json::Map::new();
    summary.insert("redacted".to_string(), serde_json::Value::Bool(true));
    let mut keys = object.keys().cloned().collect::<Vec<_>>();
    keys.sort_unstable();
    summary.insert("topLevelKeys".to_string(), serde_json::json!(keys));

    for key in [
        "api_id",
        "event_type",
        "id",
        "matched",
        "ok",
        "request_id",
        "requestId",
        "schema_id",
        "sequence",
        "status",
        "type",
    ] {
        if let Some(value) = object.get(key).filter(|value| {
            value.is_boolean() || value.is_number() || value.is_string() || value.is_null()
        }) {
            summary.insert(key.to_string(), value.clone());
        }
    }

    serde_json::Value::Object(summary)
}

fn sanitize_structured_record(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if is_sensitive_diagnostic_key(key) {
                    *value = serde_json::Value::String("[redacted]".to_string());
                } else {
                    sanitize_structured_record(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                sanitize_structured_record(value);
            }
        }
        serde_json::Value::String(value) => *value = redact_urls(value),
        _ => {}
    }
}

fn is_sensitive_diagnostic_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    matches!(
        key.as_str(),
        "auth"
            | "authorization"
            | "config"
            | "content"
            | "cookie"
            | "credentials"
            | "headers"
            | "password"
            | "private_key"
            | "psk"
            | "secret"
            | "token"
            | "uri"
            | "url"
            | "uuid"
    ) || key.ends_with("_password")
        || key.ends_with("_secret")
        || key.ends_with("_token")
        || key.ends_with("_url")
}

fn redact_urls(value: &str) -> String {
    const SENSITIVE_SCHEMES: &[&str] = &[
        "https://",
        "http://",
        "hysteria2://",
        "ss://",
        "trojan://",
        "vless://",
        "vmess://",
    ];

    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = SENSITIVE_SCHEMES
        .iter()
        .filter_map(|scheme| remaining.find(scheme))
        .min()
    {
        output.push_str(&remaining[..index]);
        output.push_str("[redacted-url]");
        let url = &remaining[index..];
        let end = url
            .char_indices()
            .find_map(|(offset, character)| {
                (offset > 0
                    && (character.is_whitespace()
                        || matches!(character, '"' | '\'' | '<' | '>' | ')' | ']' | '}')))
                .then_some(offset)
            })
            .unwrap_or(url.len());
        remaining = &url[end..];
    }
    output.push_str(remaining);
    output
}

fn write_pretty_json(path: &Path, value: &serde_json::Value) -> AppResult<()> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::internal(format!("serialize diagnostic manifest: {error}")))?;
    fs::write(path, content)
        .map_err(|error| AppError::internal(format!("write diagnostic manifest: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{redact_urls, sanitize_debug_record, sanitize_structured_record};

    #[test]
    fn diagnostic_log_sanitizer_removes_urls_and_credentials() {
        let mut record = serde_json::json!({
            "message": "request failed for https://example.com/sub?token=secret)",
            "fields": {
                "url": "https://example.com/sub?token=secret",
                "password": "secret",
                "operation": "auto_sync"
            }
        });

        sanitize_structured_record(&mut record);
        let serialized = record.to_string();
        assert!(!serialized.contains("example.com"));
        assert!(!serialized.contains("secret"));
        assert!(serialized.contains("auto_sync"));
        assert_eq!(redact_urls("ok"), "ok");
    }

    #[test]
    fn diagnostic_debug_sanitizer_keeps_envelope_but_drops_payload_content() {
        let mut record = serde_json::json!({
            "id": 9,
            "frameType": "command",
            "payload": {
                "type": "command",
                "id": "request-1",
                "matched": false,
                "request": {
                    "config": { "password": "secret" }
                }
            }
        });

        sanitize_debug_record(&mut record);
        let serialized = record.to_string();
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("secret"));
        assert_eq!(record["payload"]["type"], "command");
        assert_eq!(record["payload"]["id"], "request-1");
        assert_eq!(record["payload"]["matched"], false);
        assert_eq!(record["payload"]["redacted"], true);
    }
}
