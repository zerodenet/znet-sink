use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::{self, build_traffic_snapshot, TrafficSample, ZeroAdapter};
use crate::models::core_process::CoreProcessState;
use crate::models::gui_core::{
    ConfigProxyNode, GuiConfigPlanApplyResult, GuiConnection, GuiConnectionCloseResult,
    GuiConnectionList, GuiConnectionListOptions, GuiCoreHealth, GuiCoreOverview, GuiFeatureStatus,
    GuiPolicyGroup, GuiPolicySelectionResult, GuiTrafficSnapshot, GuiTrafficStats,
    GuiZeroCapabilities,
};
use crate::services::common;
use crate::services::{core_config, core_process, interaction_mode, probe, proxy_config};
use crate::state::app_state::AppState;

const CORE_READY_WAIT_TIMEOUT: Duration = Duration::from_secs(8);
const CORE_READY_WAIT_INTERVAL: Duration = Duration::from_millis(100);

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
        .map(|c| zero::config::outbound_kind_map(c))
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
    let opts = default_opts(state.inner());
    ZeroAdapter::new()
        .select_policy(policy_tag, target_tag, opts)
        .await
}

/// Probe a single target's connectivity.
///
/// Fire-and-forget like `gui_probe_policy`: spawns the IPC probe in background,
/// returns immediately. Results arrive via `diagnostics.probe_target` response
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
        let _ = adapter.probe_target(target_tag, opts).await;
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
pub async fn gui_tun_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    let opts = default_opts(state.inner());
    ZeroAdapter::new().tun_status(opts).await
}

#[tauri::command]
pub async fn gui_tun_enable(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiFeatureStatus> {
    ensure_core_ready(app_handle, state.clone()).await?;
    let tun = { common::lock(state.app_config(), "app_config")?.tun.clone() };
    let opts = default_opts(state.inner());
    zero::commands::enable_tun(tun.name, tun.addr, tun.tag, tun.mtu, Some(opts)).await
}

#[tauri::command]
pub async fn gui_tun_disable(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    let opts = default_opts(state.inner());
    zero::commands::disable_tun(Some(opts)).await
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
/// with kernel runtime state (selected, delay, alive) layered on top.
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

/// Probe a single node. Returns the result directly.
/// No upfront health check — if core is unavailable the probe returns an error result.
#[tauri::command]
pub async fn gui_client_probe_node(
    state: State<'_, AppState>,
    target_tag: String,
) -> AppResult<probe::ProbeResult> {
    Ok(probe::probe_single(state.inner(), &target_tag).await)
}

/// Start a batch probe. Returns immediately; results arrive via Tauri events:
/// - `probe:result`   — per-node ProbeResult
/// - `probe:progress` — `{ done, total }`
/// - `probe:complete` — `{ total, reachable, failed }`
#[tauri::command]
pub async fn gui_client_probe_start(
    app_handle: AppHandle,
    target_tags: Vec<String>,
    max_concurrent: Option<usize>,
) -> AppResult<()> {
    if target_tags.is_empty() {
        return Ok(());
    }

    let max_concurrent = max_concurrent.unwrap_or(probe::MAX_CONCURRENT_PROBES);

    // Spawn batch in background — command returns immediately
    tauri::async_runtime::spawn(probe::run_probe_batch(
        app_handle,
        target_tags,
        max_concurrent,
    ));

    Ok(())
}

/// Apply a config to the running kernel without restart (hot-reload).
#[tauri::command]
pub async fn gui_apply_config(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "apply_config")?;
    let opts = default_opts(state.inner());
    let result = ZeroAdapter::new()
        .apply_config(config.clone(), opts)
        .await?;
    // The kernel accepted the config — mirror it into the active profile so
    // that config-derived views (proxy nodes, policy groups) and the next
    // core-process start reflect the live configuration.
    let _ = proxy_config::update_active_content(state.inner(), config);
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
    ZeroAdapter::new().validate_config(config, opts).await
}

/// Dry-run config apply — returns impact analysis without applying changes.
///
/// Sends `config.plan_apply` to the kernel, which returns a structured
/// breakdown of which sections can be hot-reloaded and which require
/// a kernel restart.
#[tauri::command]
pub async fn gui_plan_apply_config(
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> AppResult<GuiConfigPlanApplyResult> {
    interaction_mode::require_pro_mode(state.inner(), "plan_apply_config")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new().plan_apply_config(config, opts).await
}

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
/// Fire-and-forget: spawns the IPC command in background, returns immediately.
/// Results arrive via the event stream as `policy.probeCompleted` events.
/// This avoids blocking the UI for the entire probe duration (which can be
/// several seconds per url_test cycle).
#[tauri::command]
pub async fn gui_probe_policy(
    state: State<'_, AppState>,
    policy_tag: String,
) -> AppResult<serde_json::Value> {
    let opts = default_opts(state.inner());
    // Fire the probe in background — the kernel will emit policy.probeCompleted
    // events when results are ready. The frontend listens for these via the
    // event subscription to update latency/health indicators.
    tauri::async_runtime::spawn(async move {
        let _ = ZeroAdapter::new().probe_policy(policy_tag, opts).await;
    });
    Ok(serde_json::json!({"accepted": true}))
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
) -> AppResult<serde_json::Value> {
    interaction_mode::require_pro_mode(state.inner(), "trace_route")?;
    let opts = default_opts(state.inner());
    ZeroAdapter::new()
        .trace_route(target, port.unwrap_or(80), protocol, opts)
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
