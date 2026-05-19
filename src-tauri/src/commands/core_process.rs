use tauri::State;

use crate::errors::AppResult;
use crate::models::core_process::CoreProcessStatus;
use crate::services::core_process;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn core_process_status(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    core_process::status(state)
}

#[tauri::command]
pub fn core_process_start(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    core_process::start(state)
}

#[tauri::command]
pub fn core_process_stop(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    core_process::stop(state)
}
