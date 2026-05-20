use tauri::State;

use crate::errors::AppResult;
use crate::models::capability::{GuiCapabilitySnapshot, InteractionSurfaceSnapshot};
use crate::services::capability;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn gui_capabilities_snapshot(state: State<'_, AppState>) -> AppResult<GuiCapabilitySnapshot> {
    capability::snapshot(state)
}

#[tauri::command]
pub async fn gui_interaction_surface_snapshot(
    state: State<'_, AppState>,
) -> AppResult<InteractionSurfaceSnapshot> {
    capability::interaction_surface(state).await
}
