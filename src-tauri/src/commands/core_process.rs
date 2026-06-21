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

/// Restart the managed kernel: stop the current process and start a new one.
/// This is the recommended way for UI to refresh the kernel — regular stop
/// is only for app shutdown.
#[tauri::command]
pub fn core_process_restart(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<CoreProcessStatus> {
    // Stop first, then start. Both are sync/blocking.
    let _ = core_process::stop(state.clone());
    core_process::start(app_handle, state)
}
