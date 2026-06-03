use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::models::core_process::CoreProcessStatus;
use crate::services::core_process;
use crate::state::app_state::AppState;

/// Fast in-memory read — can stay sync.
#[tauri::command]
pub fn core_process_status(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    core_process::status(state)
}

/// Spawns OS child process. Currently synchronous because core_process::start
/// borrows AppState internals — refactor to split config-read from spawn for
/// proper async support.
#[tauri::command]
pub fn core_process_start(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<CoreProcessStatus> {
    core_process::start(app_handle, state)
}

/// Kills OS child process. Same AppState borrow constraint as start.
#[tauri::command]
pub fn core_process_stop(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    core_process::stop(state)
}
