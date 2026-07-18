use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::models::logs::{LogAppend, LogEntry, LogPage, LogQuery};
use crate::services::logs;
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn logs_list(_state: State<'_, AppState>, query: Option<LogQuery>) -> AppResult<LogPage> {
    tauri::async_runtime::spawn_blocking(move || logs::list(query))
        .await
        .map_err(|error| AppError::internal(format!("log query worker failed: {error}")))?
}

#[tauri::command]
pub async fn logs_append(app_handle: AppHandle, input: LogAppend) -> AppResult<LogEntry> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        logs::append(state.inner(), input)
    })
    .await
    .map_err(|error| AppError::internal(format!("log append worker failed: {error}")))?
}

#[tauri::command]
pub async fn logs_clear(app_handle: AppHandle) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        logs::clear(state.inner())
    })
    .await
    .map_err(|error| AppError::internal(format!("log clear worker failed: {error}")))?
}
