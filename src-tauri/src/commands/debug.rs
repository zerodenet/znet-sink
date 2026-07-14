use tauri::State;

use crate::errors::AppResult;
use crate::models::debug::{clear_debug_frames, DebugFramePage, DebugFrameQuery};
use crate::services::debug_store;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn gui_debug_frames(
    _state: State<'_, AppState>,
    query: Option<DebugFrameQuery>,
) -> AppResult<DebugFramePage> {
    let mut query = query.unwrap_or_default();
    query.limit = Some(query.limit.unwrap_or(200).clamp(1, 1_000));
    debug_store::query_page(&query)
}

#[tauri::command]
pub fn gui_debug_clear(_state: State<'_, AppState>) -> AppResult<()> {
    clear_debug_frames();
    debug_store::clear()?;
    Ok(())
}
