use tauri::{AppHandle, Manager, State};

use crate::errors::AppResult;
use crate::models::proxy_config::{ProxyConfigImport, ProxyConfigProfile, ProxyConfigUpsert};
use crate::services::{interaction_mode, proxy_config};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn proxy_config_list(state: State<'_, AppState>) -> AppResult<Vec<ProxyConfigProfile>> {
    proxy_config::list(state)
}

#[tauri::command]
pub fn proxy_config_get(state: State<'_, AppState>, id: String) -> AppResult<ProxyConfigProfile> {
    proxy_config::get(state, id)
}

#[tauri::command]
pub async fn proxy_config_upsert(
    app_handle: AppHandle,
    input: ProxyConfigUpsert,
) -> AppResult<ProxyConfigProfile> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "proxyConfig")?;
    proxy_config::upsert_runtime(app_handle.clone(), input).await
}

#[tauri::command]
pub async fn proxy_config_import(
    app_handle: AppHandle,
    input: ProxyConfigImport,
) -> AppResult<ProxyConfigProfile> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "proxyConfig")?;
    proxy_config::import_runtime(app_handle.clone(), input).await
}

#[tauri::command]
pub async fn proxy_config_set_active(
    app_handle: AppHandle,
    id: String,
) -> AppResult<ProxyConfigProfile> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "proxyConfig")?;
    proxy_config::activate_runtime(app_handle.clone(), id).await
}

#[tauri::command]
pub async fn proxy_config_remove(app_handle: AppHandle, id: String) -> AppResult<()> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "proxyConfig")?;
    proxy_config::remove_runtime(app_handle.clone(), id).await
}
