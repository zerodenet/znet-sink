use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::models::core_process::CoreProcessStatus;
use crate::services::core_process;
use crate::state::app_state::AppState;

/// Fast in-memory read — can stay sync.
#[tauri::command]
pub fn core_process_status(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    core_process::status(state)
}

/// Spawns OS child process. Runs the blocking start routine on a background
/// thread so the UI stays responsive — `core_process::start` does file IO,
/// a kill-backoff sleep, and a port check that would otherwise stall the
/// main thread and freeze the window.
#[tauri::command]
pub async fn core_process_start(
    app_handle: AppHandle,
) -> AppResult<CoreProcessStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        core_process::start(app_handle.clone(), state)
    })
    .await
    .map_err(|e| AppError::internal(format!("core start task failed: {e}")))?
}

/// Restart the managed kernel: stop the current process and start a new one.
/// Runs on a background thread because `stop` synchronously waits on the
/// child (`child.wait()`) and joins the stderr pump — on the main thread
/// that freeze is what previously left the window "not responding" until the
/// OS killed the process.
#[tauri::command]
pub async fn core_process_restart(
    app_handle: AppHandle,
) -> AppResult<CoreProcessStatus> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let _ = core_process::stop(state.clone());
        core_process::start(app_handle.clone(), state)
    })
    .await
    .map_err(|e| AppError::internal(format!("core restart task failed: {e}")))?
}
