use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::debug::{clear_debug_frames, DebugFramePage, DebugFrameQuery};
use crate::services::debug_store;
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn gui_debug_frames(
    _state: State<'_, AppState>,
    query: Option<DebugFrameQuery>,
) -> AppResult<DebugFramePage> {
    let mut query = query.unwrap_or_default();
    query.limit = Some(query.limit.unwrap_or(200).clamp(1, 1_000));
    tauri::async_runtime::spawn_blocking(move || debug_store::query_page(&query))
        .await
        .map_err(|error| AppError::internal(format!("debug query worker failed: {error}")))?
}

#[tauri::command]
pub async fn gui_debug_clear(_state: State<'_, AppState>) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(|| {
        clear_debug_frames();
        debug_store::clear()
    })
    .await
    .map_err(|error| AppError::internal(format!("debug clear worker failed: {error}")))?
}
