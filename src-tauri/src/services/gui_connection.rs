use std::time::{Duration, Instant};
use tauri::{AppHandle, State};

use crate::errors::{AppError, AppResult};
use crate::models::{
    core_process::CoreProcessState,
    gui_core::{GuiConnectionStatus, GuiCoreHealth},
};
use crate::services::{
    common::lock, core_config, core_process, local_proxy, system_proxy, system_proxy_guard,
    zero_adapter,
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
    let active_proxy_config_id = active_proxy_config_id(state.inner())?;
    core_config::export_active(state.clone())?;

    let managed_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let existing_core_ready = if managed_running {
        true
    } else {
        zero_adapter::core_readiness_health(state.inner())
            .await
            .is_ok()
    };

    let mut started_this_call = false;
    if !existing_core_ready {
        let process = core_process::start(app_handle, state.clone())?;
        started_this_call = process.state == CoreProcessState::Running;
        if process.state != CoreProcessState::Running {
            return build_status(
                state.inner(),
                "failed",
                Some("core process did not enter running state".to_string()),
            )
            .await;
        }
    }

    let health = match wait_for_health(state.inner()).await {
        Ok(health) => health,
        Err(error) => {
            cleanup_failed_connect(state.clone(), started_this_call);
            return build_status(
                state.inner(),
                "failed",
                Some(format!("core readiness check failed: {}", error.message)),
            )
            .await;
        }
    };
    if !health.healthy {
        cleanup_failed_connect(state.clone(), started_this_call);
        return build_status(
            state.inner(),
            "failed",
            Some("core health check reported unhealthy".to_string()),
        )
        .await;
    }

    let (host, port) = local_proxy_endpoint(state.inner())?;
    if let Err(error) = tauri::async_runtime::spawn_blocking({
        let host = host.clone();
        move || local_proxy::wait_until_listening(&host, port)
    })
    .await
    .map_err(|error| AppError::internal(format!("local proxy probe thread panicked: {error}")))?
    {
        cleanup_failed_connect(state.clone(), started_this_call);
        return build_status(
            state.inner(),
            "failed",
            Some(format!(
                "local proxy endpoint is not ready: {}",
                error.message
            )),
        )
        .await;
    }

    if let Err(error) = system_proxy_guard::enable_with_guard(&host, port) {
        cleanup_failed_connect(state.clone(), started_this_call);
        return build_status(
            state.inner(),
            "failed",
            Some(format!("failed to enable system proxy: {}", error.message)),
        )
        .await;
    }

    build_status(state.inner(), "connected", None)
        .await
        .map(|status| GuiConnectionStatus {
            active_proxy_config_id,
            ..status
        })
}

pub async fn disconnect(state: State<'_, AppState>) -> AppResult<GuiConnectionStatus> {
    let proxy_result = system_proxy_guard::disable_with_guard();
    let stop_result = core_process::stop(state.clone());

    let error = proxy_result
        .err()
        .map(|error| error.message)
        .or_else(|| stop_result.err().map(|error| error.message));

    build_status(
        state.inner(),
        if error.is_some() {
            "failed"
        } else {
            "disconnected"
        },
        error,
    )
    .await
}

async fn build_status(
    state: &AppState,
    stage: &'static str,
    error: Option<String>,
) -> AppResult<GuiConnectionStatus> {
    let process = core_process::refresh_status(state)?;
    let health = zero_adapter::core_readiness_health(state).await.ok();
    let stats = zero_adapter::traffic_stats(state).await.unwrap_or_default();
    let active_proxy_config_id = active_proxy_config_id(state).ok().flatten();
    let (local_proxy_host, local_proxy_port) = local_proxy_endpoint(state)?;
    let core_available = process.state == CoreProcessState::Running
        || health.as_ref().is_some_and(|health| health.healthy);
    let mut system_proxy = system_proxy::status().ok();
    if !core_available
        && system_proxy.as_ref().is_some_and(|proxy| {
            proxy.enabled && proxy.host == local_proxy_host && proxy.port == local_proxy_port
        })
    {
        let _ = system_proxy_guard::disable_with_guard();
        system_proxy = system_proxy::status().ok();
    }
    let connected = core_available
        && health.as_ref().is_some_and(|health| health.healthy)
        && system_proxy.as_ref().is_some_and(|proxy| {
            proxy.enabled && proxy.host == local_proxy_host && proxy.port == local_proxy_port
        });

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

fn cleanup_failed_connect(state: State<'_, AppState>, started_this_call: bool) {
    if started_this_call {
        let _ = core_process::stop(state);
    }
}

async fn wait_for_health(state: &AppState) -> AppResult<GuiCoreHealth> {
    // Give the core a moment to create its IPC pipe before we start hammering it.
    // Without this, WaitNamedPipeW blocks for the full per-attempt timeout on every
    // call while the pipe doesn't exist yet, burning through the retry window.
    let _ = tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(HEALTH_WAIT_INITIAL_DELAY);
    })
    .await;

    let started = Instant::now();
    let mut last_error = None;

    while started.elapsed() < HEALTH_WAIT_TIMEOUT {
        match zero_adapter::core_readiness_health(state).await {
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
