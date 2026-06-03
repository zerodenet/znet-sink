use tauri::State;

use crate::errors::AppResult;
use crate::services::common::lock;
use crate::services::system_proxy::{self, SystemProxyStatus};
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn system_proxy_enable(state: State<'_, AppState>) -> AppResult<SystemProxyStatus> {
    let host = { lock(state.app_config(), "app_config")?.local_proxy.host.clone() };
    let port = { lock(state.app_config(), "app_config")?.local_proxy.port };
    tauri::async_runtime::spawn_blocking(move || system_proxy::enable(&host, port))
        .await
        .map_err(|e| crate::errors::AppError::internal(format!("system proxy thread panicked: {e}")))?
}

#[tauri::command]
pub async fn system_proxy_disable() -> AppResult<SystemProxyStatus> {
    tauri::async_runtime::spawn_blocking(|| system_proxy::disable())
        .await
        .map_err(|e| crate::errors::AppError::internal(format!("system proxy thread panicked: {e}")))?
}

#[tauri::command]
pub async fn system_proxy_status() -> AppResult<SystemProxyStatus> {
    tauri::async_runtime::spawn_blocking(|| system_proxy::status())
        .await
        .map_err(|e| crate::errors::AppError::internal(format!("system proxy thread panicked: {e}")))?
}
