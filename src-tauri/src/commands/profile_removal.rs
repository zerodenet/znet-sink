use crate::{
    errors::AppResult,
    models::profile_removal::{RemovalInput, RemovalPreview, RemovalResult},
    services::profile_removal,
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, State, Window};
#[tauri::command]
pub async fn profile_removal_preview(
    state: State<'_, AppState>,
    id: String,
    window: Window,
) -> AppResult<RemovalPreview> {
    profile_removal::require_host(window.label())?;
    let _guard = state.proxy_config_operation().lock().await;
    profile_removal::preview(state.inner(), id).await
}
#[tauri::command]
pub async fn profile_removal_commit(
    app_handle: AppHandle,
    input: RemovalInput,
    window: Window,
) -> AppResult<RemovalResult> {
    profile_removal::require_host(window.label())?;
    let state = app_handle.state::<AppState>();
    crate::services::interaction_mode::require_pro_mode(state.inner(), "proxyConfig")?;
    let _guard = state.proxy_config_operation().lock().await;
    profile_removal::commit(app_handle.clone(), input).await
}
#[tauri::command]
pub async fn profile_removal_reconcile(
    state: State<'_, AppState>,
    id: String,
    window: Window,
) -> AppResult<RemovalResult> {
    profile_removal::require_host(window.label())?;
    let _guard = state.proxy_config_operation().lock().await;
    profile_removal::reconcile(state.inner(), id).await
}
