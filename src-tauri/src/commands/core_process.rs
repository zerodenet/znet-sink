use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::runtime_performance::{self, RuntimePerformanceSnapshot};
use crate::errors::{AppError, AppResult};
use crate::kernel::zero;
use crate::models::core_process::{CoreProcessState, CoreProcessStatus};
use crate::services::{common, core_config, core_process};
use crate::state::app_state::AppState;

mod tun_restore;

const TUN_RESTORE_TIMEOUT: Duration = Duration::from_secs(8);
const TUN_RESTORE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreProcessTransitionResponse {
    #[serde(flatten)]
    pub status: CoreProcessStatus,
    pub tun_restore_error: Option<AppError>,
}

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

pub(crate) async fn app_tun_runtime_enabled(state: &AppState) -> AppResult<bool> {
    if active_profile_defines_tun(state)? {
        return Ok(false);
    }
    let app_config = common::lock(state.app_config(), "app_config")?.clone();
    let options = core_config::ipc_options_from_app_config(&app_config.core);
    zero::runtime::tun_status(Some(options))
        .await
        .map(|status| status.enabled)
}

pub(crate) async fn restore_app_tun_after_core_transition_with_desired(
    state: &AppState,
    desired_enabled: Option<bool>,
) -> AppResult<()> {
    let app_config = common::lock(state.app_config(), "app_config")?.clone();
    let should_enable = desired_enabled.unwrap_or(app_config.tun.enabled == Some(true));
    if !should_enable || active_profile_defines_tun(state)? {
        return Ok(());
    }

    let options = core_config::ipc_options_from_app_config(&app_config.core);
    tun_restore::restore(
        || zero::runtime::tun_status(Some(options.clone())),
        || zero::runtime::enable_tun(app_config.tun.clone(), Some(options.clone())),
        TUN_RESTORE_TIMEOUT,
        TUN_RESTORE_INTERVAL,
    )
    .await
}

pub(crate) async fn restore_app_tun_after_core_transition(state: &AppState) -> AppResult<()> {
    restore_app_tun_after_core_transition_with_desired(state, None).await
}

async fn restore_app_tun_best_effort(
    state: &AppState,
    transition: &'static str,
) -> Option<AppError> {
    let error = restore_app_tun_after_core_transition(state).await.err();
    if let Some(error) = &error {
        crate::services::file_logger::line(&format!(
            "failed to restore persisted app-owned TUN after Core transition: transition={transition} code={} error={}",
            error.code, error.message
        ));
    }
    error
}

/// Fast in-memory process state read. Resource metrics are opt-in so existing
/// status polling stays as cheap as before. The desktop monitor intentionally
/// samples CPU time and RSS only; thread enumeration is not part of the runtime
/// monitor because it adds platform-specific cost without useful product value.
#[tauri::command(rename_all = "camelCase")]
pub fn core_process_status(
    state: State<'_, AppState>,
    include_performance: Option<bool>,
    // Kept as a compatibility argument for callers built against an earlier
    // #20 revision. Thread sampling is intentionally ignored.
    include_performance_threads: Option<bool>,
) -> AppResult<CoreProcessStatusResponse> {
    let _ = include_performance_threads;
    let runtime_performance = if include_performance.unwrap_or(false) {
        Some(runtime_performance::runtime_performance_snapshot(
            state.clone(),
            false,
        )?)
    } else {
        None
    };
    let mut status = core_process::status(state)?;
    if status.state != CoreProcessState::Running {
        status.pid = None;
    }
    Ok(CoreProcessStatusResponse {
        status,
        runtime_performance,
    })
}

#[tauri::command]
pub async fn core_process_start(app_handle: AppHandle) -> AppResult<CoreProcessTransitionResponse> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let start_app = app_handle.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let state = start_app.state::<AppState>();
        core_process::start(start_app.clone(), state)
    })
    .await
    .map_err(|e| AppError::internal(format!("core start task failed: {e}")))??;

    let tun_restore_error = restore_app_tun_best_effort(state.inner(), "start").await;
    Ok(CoreProcessTransitionResponse {
        status,
        tun_restore_error,
    })
}

#[tauri::command]
pub async fn core_process_restart(
    app_handle: AppHandle,
) -> AppResult<CoreProcessTransitionResponse> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let restart_app = app_handle.clone();
    let status = tauri::async_runtime::spawn_blocking(move || core_process::restart(restart_app))
        .await
        .map_err(|e| AppError::internal(format!("core restart task failed: {e}")))??;

    let tun_restore_error = restore_app_tun_best_effort(state.inner(), "restart").await;
    Ok(CoreProcessTransitionResponse {
        status,
        tun_restore_error,
    })
}
