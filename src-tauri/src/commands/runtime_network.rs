use crate::{
    configuration::runtime_network,
    errors::{AppError, AppResult},
    models::runtime_network::{Plan, PreviewInput, Snapshot, Status},
    services::{interaction_mode, native_documents},
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, Window};

fn host(state: &AppState, label: &str) -> AppResult<()> {
    native_documents::require_host(label)?;
    interaction_mode::require_pro_mode(state, "runtime_network")
}
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    label: String,
    run: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        host(state.inner(), &label)?;
        run(state.inner())
    })
    .await
    .map_err(|_| AppError::internal("网络设置任务未完成；草稿保留，请只读核对原操作"))?
}
#[tauri::command]
pub async fn gui_runtime_network_snapshot(
    app_handle: AppHandle,
    window: Window,
) -> AppResult<Snapshot> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), runtime_network::snapshot).await
}
#[tauri::command]
pub async fn gui_runtime_network_preview(
    app_handle: AppHandle,
    window: Window,
    input: PreviewInput,
) -> AppResult<Plan> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        runtime_network::preview(state, input)
    })
    .await
}
#[tauri::command]
pub async fn gui_runtime_network_save(
    app_handle: AppHandle,
    window: Window,
    plan_id: String,
) -> AppResult<Status> {
    let state = app_handle.state::<AppState>();
    host(state.inner(), window.label())?;
    let _operation = state.proxy_config_operation().lock().await;
    blocking(app_handle.clone(), window.label().into(), move |state| {
        runtime_network::save(state, &plan_id)
    })
    .await
}
#[tauri::command]
pub async fn gui_runtime_network_status(
    app_handle: AppHandle,
    window: Window,
    plan_id: String,
) -> AppResult<Status> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        runtime_network::status(state, &plan_id)
    })
    .await
}
#[tauri::command]
pub async fn gui_runtime_network_discard(
    app_handle: AppHandle,
    window: Window,
    plan_id: String,
) -> AppResult<()> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    blocking(app_handle.clone(), window.label().into(), move |state| {
        runtime_network::discard(state, &plan_id)
    })
    .await
}
