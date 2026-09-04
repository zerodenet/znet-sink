use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::debug::{clear_debug_frames, DebugFramePage, DebugFrameQuery};
use crate::services::{connection_history_store, debug_store};
use crate::state::app_state::AppState;

const CONNECTION_HISTORY_SCOPE: &str = "connection-history";

#[tauri::command]
pub async fn gui_debug_frames(
    _state: State<'_, AppState>,
    query: Option<DebugFrameQuery>,
) -> AppResult<DebugFramePage> {
    let mut query = query.unwrap_or_default();
    let is_connection_history = query.frame_type.as_deref() == Some(CONNECTION_HISTORY_SCOPE);
    query.limit = Some(
        query
            .limit
            .unwrap_or(if is_connection_history { 50 } else { 200 })
            .clamp(1, if is_connection_history { 200 } else { 1_000 }),
    );

    tauri::async_runtime::spawn_blocking(move || {
        if is_connection_history {
            query.frame_type = None;
            connection_history_store::query_page(&query)
        } else {
            debug_store::query_page(&query)
        }
    })
    .await
    .map_err(|error| AppError::internal(format!("debug query worker failed: {error}")))?
}

#[tauri::command]
pub async fn gui_debug_clear(_state: State<'_, AppState>, scope: Option<String>) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        if scope.as_deref() == Some(CONNECTION_HISTORY_SCOPE) {
            return connection_history_store::clear();
        }

        clear_debug_frames();
        debug_store::clear()?;
        connection_history_store::clear()
    })
    .await
    .map_err(|error| AppError::internal(format!("debug clear worker failed: {error}")))?
}
