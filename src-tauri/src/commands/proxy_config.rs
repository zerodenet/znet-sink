use tauri::State;

use crate::errors::AppResult;
use crate::models::proxy_config::{ProxyConfigImport, ProxyConfigProfile, ProxyConfigUpsert};
use crate::services::proxy_config;
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
pub fn proxy_config_upsert(
    state: State<'_, AppState>,
    input: ProxyConfigUpsert,
) -> AppResult<ProxyConfigProfile> {
    proxy_config::upsert(state, input)
}

#[tauri::command]
pub fn proxy_config_import(
    state: State<'_, AppState>,
    input: ProxyConfigImport,
) -> AppResult<ProxyConfigProfile> {
    proxy_config::import(state, input)
}

#[tauri::command]
pub fn proxy_config_set_active(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<ProxyConfigProfile> {
    proxy_config::set_active(state, id)
}

#[tauri::command]
pub fn proxy_config_remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    proxy_config::remove(state, id)
}
