use tauri::State;

use crate::errors::AppResult;
use crate::models::debug::{clear_debug_frames, snapshot_debug_frames, DebugFrame};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn gui_debug_frames(_state: State<'_, AppState>) -> AppResult<Vec<DebugFrame>> {
    Ok(snapshot_debug_frames())
}

#[tauri::command]
pub fn gui_debug_clear(_state: State<'_, AppState>) -> AppResult<()> {
    clear_debug_frames();
    Ok(())
}
