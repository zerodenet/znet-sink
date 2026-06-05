use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::core::CoreIpcOptions;
use crate::models::core_process::CoreProcessState;
use crate::services::common::lock;
use crate::services::core_config;
use crate::services::system_proxy::{self, SystemProxyStatus};
use crate::services::{core_process, local_proxy, system_proxy_guard};
use crate::state::app_state::AppState;

const CORE_READY_WAIT_TIMEOUT: Duration = Duration::from_secs(8);
const CORE_READY_WAIT_INTERVAL: Duration = Duration::from_millis(100);

fn default_ipc_opts(state: &AppState) -> CoreIpcOptions {
    core_config::ipc_options_from_app_config(
        &lock(state.app_config(), "app_config")
            .map(|c| c.core.clone())
            .unwrap_or_default(),
    )
}

#[tauri::command]
pub async fn system_proxy_enable(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<SystemProxyStatus> {
    ensure_core_ready(app_handle, state.clone()).await?;

    let host = {
        lock(state.app_config(), "app_config")?
            .local_proxy
            .host
            .clone()
    };
    let port = { lock(state.app_config(), "app_config")?.local_proxy.port };
    tauri::async_runtime::spawn_blocking(move || {
        local_proxy::wait_until_listening(&host, port)?;
        system_proxy_guard::enable_with_guard(&host, port)?;
        system_proxy::status()
    })
    .await
    .map_err(|e| crate::errors::AppError::internal(format!("system proxy thread panicked: {e}")))?
}

#[tauri::command]
pub async fn system_proxy_disable() -> AppResult<SystemProxyStatus> {
    tauri::async_runtime::spawn_blocking(|| {
        system_proxy_guard::disable_with_guard()?;
        system_proxy::status()
    })
    .await
    .map_err(|e| crate::errors::AppError::internal(format!("system proxy thread panicked: {e}")))?
}

#[tauri::command]
pub async fn system_proxy_status() -> AppResult<SystemProxyStatus> {
    tauri::async_runtime::spawn_blocking(|| system_proxy::status())
        .await
        .map_err(|e| {
            crate::errors::AppError::internal(format!("system proxy thread panicked: {e}"))
        })?
}

async fn ensure_core_ready(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let adapter = ZeroAdapter::new();
    let opts = default_ipc_opts(state.inner());
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
        let opts = default_ipc_opts(state);
        match adapter.readiness_health(opts).await {
            Ok(health) if health.healthy => return Ok(()),
            Ok(_) => {
                return Err(AppError::internal(
                    "core readiness check reported unhealthy",
                ));
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
