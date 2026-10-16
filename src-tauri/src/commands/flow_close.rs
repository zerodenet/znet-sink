use crate::{
    errors::AppResult,
    models::flow_close::{Inventory, Preview, PreviewInput, ResultView, Status},
    services::{flow_close, interaction_mode, native_documents},
    state::app_state::AppState,
};
use tauri::{State, Window};
fn host(state: &AppState, window: &Window) -> AppResult<()> {
    native_documents::require_host(window.label())?;
    interaction_mode::require_pro_mode(state, "connections")
}
#[tauri::command]
pub async fn gui_flow_close_inventory(
    state: State<'_, AppState>,
    window: Window,
) -> AppResult<Inventory> {
    host(state.inner(), &window)?;
    flow_close::inventory(state.inner()).await
}
#[tauri::command]
pub async fn gui_flow_close_preview(
    state: State<'_, AppState>,
    window: Window,
    input: PreviewInput,
) -> AppResult<Preview> {
    host(state.inner(), &window)?;
    flow_close::preview(state.inner(), input).await
}
#[tauri::command]
pub async fn gui_flow_close_submit(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<ResultView> {
    host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    flow_close::submit(state.inner(), &plan_id).await
}
#[tauri::command]
pub fn gui_flow_close_status(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<Status> {
    host(state.inner(), &window)?;
    flow_close::status(state.inner(), &plan_id)
}
#[tauri::command]
pub fn gui_flow_close_discard(
    state: State<'_, AppState>,
    window: Window,
    id: String,
) -> AppResult<()> {
    host(state.inner(), &window)?;
    flow_close::discard(state.inner(), &id)
}
