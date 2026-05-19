use tauri::State;

use crate::errors::AppResult;
use crate::models::logs::{LogAppend, LogEntry, LogQuery};
use crate::services::logs;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn logs_list(state: State<'_, AppState>, query: Option<LogQuery>) -> AppResult<Vec<LogEntry>> {
    logs::list(state, query)
}

#[tauri::command]
pub fn logs_append(state: State<'_, AppState>, input: LogAppend) -> AppResult<LogEntry> {
    logs::append(state, input)
}

#[tauri::command]
pub fn logs_clear(state: State<'_, AppState>) -> AppResult<()> {
    logs::clear(state)
}
