use tauri::State;

use crate::errors::AppResult;
use crate::models::capability::GuiCapabilitySnapshot;
use crate::services::capability;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn gui_capabilities_snapshot(state: State<'_, AppState>) -> AppResult<GuiCapabilitySnapshot> {
    capability::snapshot(state)
}
