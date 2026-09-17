use std::time::Duration;
use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::models::gui_core::{GuiProxyModeStatus, GuiSetProxyModeInput};
use crate::services::proxy_mode;
use crate::state::app_state::AppState;

#[tauri::command]
pub fn gui_proxy_mode_status(state: State<'_, AppState>) -> AppResult<GuiProxyModeStatus> {
    proxy_mode::status(state.inner())
}

#[tauri::command]
pub async fn gui_set_proxy_mode(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: GuiSetProxyModeInput,
) -> AppResult<GuiProxyModeStatus> {
    let scope = format!("{:?}", input.mode);
    crate::services::native_operation::execute_async(
        state.inner(),
        "runtime.proxy-mode",
        &scope,
        Duration::from_secs(120),
        proxy_mode::set(app_handle, state.clone(), input),
    )
    .await
}
