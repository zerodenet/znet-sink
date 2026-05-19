use tauri::State;

use crate::errors::AppResult;
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::services::app_config;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn app_config_get(state: State<'_, AppState>) -> AppResult<AppConfig> {
    app_config::get(state)
}

#[tauri::command]
pub fn app_config_update(
    state: State<'_, AppState>,
    patch: AppConfigPatch,
) -> AppResult<AppConfig> {
    app_config::update(state, patch)
}
