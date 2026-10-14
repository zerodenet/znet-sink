use crate::{
    errors::AppResult,
    services::{native_documents, runtime_bridge},
    state::app_state::AppState,
};
use tauri::{State, Window};

/// Read-only main-window inspection. No raw query, command or caller arguments.
#[tauri::command]
pub async fn gui_runtime_bridge_inspect(
    state: State<'_, AppState>,
    window: Window,
) -> AppResult<runtime_bridge::Inspection> {
    native_documents::require_host(window.label())?;
    runtime_bridge::inspect(state.inner(), window.label()).await
}
