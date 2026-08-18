use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::runtime_performance::{self, RuntimePerformanceSnapshot};
use crate::errors::{AppError, AppResult};
use crate::kernel::zero;
use crate::models::core_process::{CoreProcessState, CoreProcessStatus};
use crate::services::{common, core_config, core_process};
use crate::state::app_state::AppState;

const TUN_RESTORE_TIMEOUT: Duration = Duration::from_secs(8);
const TUN_RESTORE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreProcessStatusResponse {
    #[serde(flatten)]
    pub status: CoreProcessStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_performance: Option<RuntimePerformanceSnapshot>,
}

fn active_profile_defines_tun(state: &AppState) -> AppResult<bool> {
    Ok(common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.as_ref())
        .and_then(|content| content.get("runtime"))
        .and_then(serde_json::Value::as_object)
        .is_some_and(|runtime| runtime.contains_key("tun")))
}

async fn restore_app_tun_after_core_transition(state: &AppState) -> AppResult<()> {
    let app_config = common::lock(state.app_config(), "app_config")?.clone();
    if app_config.tun.enabled != Some(true) || active_profile_defines_tun(state)? {
        return Ok(());
    }

    let options = core_config::ipc_options_from_app_config(&app_config.core);
    let deadline = tokio::time::Instant::now() + TUN_RESTORE_TIMEOUT;
    loop {
        match zero::runtime::tun_status(Some(options.clone())).await {
            Ok(status) => {
                if status.enabled {
                    return Ok(());
                }
                if !status.supported {
                    return Err(AppError::invalid_argument(
                        "the current Zero runtime does not support TUN",
                    ));
                }
                zero::runtime::enable_tun(app_config.tun.clone(), Some(options)).await?;
                return Ok(());
            }
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(TUN_RESTORE_INTERVAL).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn restore_app_tun_best_effort(state: &AppState, transition: &'static str) {
    if let Err(error) = restore_app_tun_after_core_transition(state).await {
        crate::services::file_logger::line(&format!(
            "failed to restore persisted app-owned TUN after Core transition: transition={transition} code={} error={}",
            error.code, error.message
        ));
    }
}

/// Fast in-memory process state read. Resource metrics are opt-in so existing
/// status polling stays as cheap as before; the overview requests them once
/// per second while it is visible.
#[tauri::command(rename_all = "camelCase")]
pub fn core_process_status(
    state: State<'_, AppState>,
    include_performance: Option<bool>,
) -> AppResult<CoreProcessStatusResponse> {
    let runtime_performance = if include_performance.unwrap_or(false) {
        Some(runtime_performance::runtime_performance_snapshot(state.clone())?)
    } else {
        None
    };
    let mut status = core_process::status(state)?;
    // PID identifies the currently managed child only. Never expose a stale
    // identifier retained by an exited/failed transition as if it were live.
    if status.state != CoreProcessState::Running {
        status.pid = None;
    }
    Ok(CoreProcessStatusResponse {
        status,
        runtime_performance,
    })
}

/// Spawns OS child process. Runs the blocking start routine on a background
/// thread so the UI stays responsive — `core_process::start` does file IO,
/// a kill-backoff sleep, and a port check that would otherwise stall the
/// main thread and freeze the window.
#[tauri::command]
pub async fn core_process_start(app_handle: AppHandle) -> AppResult<CoreProcessStatus> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let start_app = app_handle.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let state = start_app.state::<AppState>();
        core_process::start(start_app.clone(), state)
    })
    .await
    .map_err(|e| AppError::internal(format!("core start task failed: {e}")))??;

    restore_app_tun_best_effort(state.inner(), "start").await;
    Ok(status)
}

/// Restart the managed kernel: stop the current process and start a new one.
/// Runs on a background thread because `stop` synchronously waits on the
/// child (`child.wait()`) and joins the stderr pump — on the main thread
/// that freeze is what previously left the window "not responding" until the
/// OS killed the process.
#[tauri::command]
pub async fn core_process_restart(app_handle: AppHandle) -> AppResult<CoreProcessStatus> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let restart_app = app_handle.clone();
    let status = tauri::async_runtime::spawn_blocking(move || core_process::restart(restart_app))
        .await
        .map_err(|e| AppError::internal(format!("core restart task failed: {e}")))??;

    restore_app_tun_best_effort(state.inner(), "restart").await;
    Ok(status)
}
