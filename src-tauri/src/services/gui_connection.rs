use std::time::{Duration, Instant};
use tauri::{AppHandle, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::{
    core_process::CoreProcessState,
    gui_core::{GuiConnectionStatus, GuiCoreHealth},
};
use crate::services::{
    common::lock, core_config, core_process, local_proxy, network_probe, system_proxy,
    system_proxy_guard,
};
use crate::state::app_state::AppState;

const HEALTH_WAIT_TIMEOUT: Duration = Duration::from_secs(8);
const HEALTH_WAIT_INITIAL_DELAY: Duration = Duration::from_millis(300);
const HEALTH_WAIT_INTERVAL: Duration = Duration::from_millis(100);

pub async fn status(state: &AppState) -> AppResult<GuiConnectionStatus> {
    build_status(state, "status", None).await
}

pub async fn connect(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiConnectionStatus> {
    let _operation = state.proxy_config_operation().lock().await;
    let active_proxy_config_id = active_proxy_config_id(state.inner())?;
    core_config::export_active(state.clone())?;

    let managed_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;

    if managed_running {
        // We already manage a running core — no need to start another one.
    } else {
        let process = core_process::start(app_handle.clone(), state.clone())?;
        if process.state != CoreProcessState::Running {
            return Err(AppError::internal(
                "core process did not enter running state",
            ));
        }
    }

    let health = match wait_for_health(state.inner()).await {
        Ok(health) => health,
        Err(error) => {
            cleanup_failed_connect(state.clone());
            return Err(AppError::internal(format!(
                "core readiness check failed: {}",
                error.message
            )));
        }
    };
    if !health.healthy {
        cleanup_failed_connect(state.clone());
        return Err(AppError::internal("core health check reported unhealthy"));
    }

    let (host, port) = local_proxy_endpoint(state.inner())?;
    let bypass = lock(state.app_config(), "app_config")?
        .local_proxy
        .bypass
        .clone();
    if let Err(error) = tauri::async_runtime::spawn_blocking({
        let host = host.clone();
        move || local_proxy::wait_until_listening(&host, port)
    })
    .await
    .map_err(|error| AppError::internal(format!("local proxy probe thread panicked: {error}")))?
    {
        cleanup_failed_connect(state.clone());
        return Err(AppError::internal(format!(
            "local proxy endpoint is not ready: {}",
            error.message
        )));
    }

    if let Err(error) = system_proxy_guard::enable_with_guard_and_bypass(&host, port, &bypass) {
        cleanup_failed_connect(state.clone());
        return Err(AppError::internal(format!(
            "failed to enable system proxy: {}",
            error.message
        )));
    }

    let status = build_status(state.inner(), "connected", None)
        .await
        .map(|status| GuiConnectionStatus {
            active_proxy_config_id,
            ..status
        })?;
    if !status.connected {
        cleanup_failed_connect(state.clone());
        return Err(AppError::internal(
            status.last_error.clone().unwrap_or_else(|| {
                "system proxy did not enter the managed connected state".to_string()
            }),
        ));
    }

    network_probe::emit_host_network_changed(&app_handle, "system_proxy.enabled");
    Ok(status)
}

pub async fn disconnect(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiConnectionStatus> {
    // Disconnect is intentionally proxy-only. The managed kernel remains
    // available so Lite can reconnect quickly; kernel lifecycle is not tied
    // to the big proxy switch.
    system_proxy_guard::disable_with_guard()?;

    let status = build_status(state.inner(), "disconnected", None).await?;
    network_probe::emit_host_network_changed(&app_handle, "system_proxy.disabled");
    Ok(status)
}

async fn build_status(
    state: &AppState,
    stage: &'static str,
    error: Option<String>,
) -> AppResult<GuiConnectionStatus> {
    let mut process = core_process::refresh_status(state)?;
    // PID is the identity of the Child currently managed by this GUI, not a
    // historical process identifier and not a health-probe result. A watchdog
    // transition can briefly leave a stale pid in its internal status after
    // the child is gone; never project that stale identity into GUI state.
    if process.state != CoreProcessState::Running {
        process.pid = None;
    }

    let adapter = ZeroAdapter::new();
    let opts = default_ipc_opts(state);
    let health = adapter.readiness_health(opts).await.ok();
    let opts = default_ipc_opts(state);
    let stats = adapter.traffic_stats(opts).await.unwrap_or_default();
    let active_proxy_config_id = active_proxy_config_id(state).ok().flatten();
    let (local_proxy_host, local_proxy_port) = local_proxy_endpoint(state)?;
    let core_available = process.state == CoreProcessState::Running
        || health.as_ref().is_some_and(|health| health.healthy);
    let mut system_proxy = system_proxy::status().ok();
    let mut system_proxy_owned = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);

    // If our guarded system proxy survives while the local core is no longer
    // available, restore the user's previous proxy immediately. A raw OS proxy
    // that the user configured independently is never treated as ours here.
    if !core_available && system_proxy_owned {
        let _ = system_proxy_guard::disable_with_guard();
        system_proxy = system_proxy::status().ok();
        system_proxy_owned = false;
    }

    let connected = core_available
        && health.as_ref().is_some_and(|health| health.healthy)
        && system_proxy_owned;

    // GuiConnectionStatus describes the GUI-managed connection, not the raw
    // Windows proxy registry. Keep host/port for diagnostics, but expose
    // `enabled` as ownership by the crash-safe guard. Raw OS status remains
    // available through the dedicated system_proxy_status command.
    if let Some(proxy) = system_proxy.as_mut() {
        proxy.enabled = system_proxy_owned;
    }

    Ok(GuiConnectionStatus {
        connected,
        stage: stage.to_string(),
        core_available,
        process,
        system_proxy,
        health,
        stats,
        active_proxy_config_id,
        local_proxy_host,
        local_proxy_port,
        last_error: error,
    })
}

fn cleanup_failed_connect(_state: State<'_, AppState>) {
    let _ = system_proxy_guard::disable_with_guard();
}

async fn wait_for_health(state: &AppState) -> AppResult<GuiCoreHealth> {
    // Give the core a moment to create its IPC pipe before we start hammering it.
    let _ = tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(HEALTH_WAIT_INITIAL_DELAY);
    })
    .await;

    let started = Instant::now();
    let mut last_error = None;

    let adapter = ZeroAdapter::new();
    while started.elapsed() < HEALTH_WAIT_TIMEOUT {
        let opts = default_ipc_opts(state);
        match adapter.readiness_health(opts).await {
            Ok(health) if health.healthy => return Ok(health),
            Ok(health) => return Ok(health),
            Err(error) => {
                last_error = Some(error);
                let _ = tauri::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(HEALTH_WAIT_INTERVAL);
                })
                .await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::internal("core health check timed out")))
}

fn active_proxy_config_id(state: &AppState) -> AppResult<Option<String>> {
    Ok(lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .map(|profile| profile.id.clone()))
}

fn local_proxy_endpoint(state: &AppState) -> AppResult<(String, u16)> {
    let config = lock(state.app_config(), "app_config")?;
    Ok((config.local_proxy.host.clone(), config.local_proxy.port))
}

fn default_ipc_opts(state: &AppState) -> crate::models::core::CoreIpcOptions {
    core_config::ipc_options_from_app_config(
        &lock(state.app_config(), "app_config")
            .map(|c| c.core.clone())
            .unwrap_or_default(),
    )
}
