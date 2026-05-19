use tauri::State;

use crate::errors::AppResult;
use crate::models::core_config::{CoreConfigExportResult, CoreConfigSnapshot};
use crate::services::core_config;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn core_config_get(state: State<'_, AppState>) -> AppResult<CoreConfigSnapshot> {
    core_config::snapshot(state)
}

#[tauri::command]
pub fn core_config_export_active(state: State<'_, AppState>) -> AppResult<CoreConfigExportResult> {
    core_config::export_active(state)
}
