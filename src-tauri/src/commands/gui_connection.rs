use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::models::gui_core::GuiConnectionStatus;
use crate::services::gui_connection;
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn gui_connection_status(state: State<'_, AppState>) -> AppResult<GuiConnectionStatus> {
    gui_connection::status(state.inner()).await
}

#[tauri::command]
pub async fn gui_connect(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiConnectionStatus> {
    gui_connection::connect(app_handle, state).await
}

#[tauri::command]
pub async fn gui_disconnect(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiConnectionStatus> {
    gui_connection::disconnect(app_handle, state).await
}
