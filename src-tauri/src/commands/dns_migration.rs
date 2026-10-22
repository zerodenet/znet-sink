use crate::{
    configuration::dns_migration,
    errors::{AppError, AppResult},
    models::dns_migration::{Plan, PreviewInput, Snapshot, Status},
    services::{interaction_mode, native_documents},
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, Window};

fn host(state: &AppState, label: &str) -> AppResult<()> {
    native_documents::require_host(label)?;
    interaction_mode::require_pro_mode(state, "dns_migration")
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
    .map_err(|_| AppError::internal("DNS 迁移任务未完成；草稿保留，请只读核对原操作"))?
}
#[tauri::command]
pub async fn gui_dns_migration_snapshot(
    app_handle: AppHandle,
    window: Window,
    profile_id: String,
) -> AppResult<Snapshot> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        dns_migration::snapshot(state, &profile_id)
    })
    .await
}
#[tauri::command]
pub async fn gui_dns_migration_preview(
    app_handle: AppHandle,
    window: Window,
    input: PreviewInput,
) -> AppResult<Plan> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        dns_migration::preview(state, input)
    })
    .await
}
#[tauri::command]
pub async fn gui_dns_migration_save(
    app_handle: AppHandle,
    window: Window,
    plan_id: String,
) -> AppResult<Status> {
    let state = app_handle.state::<AppState>();
    host(state.inner(), window.label())?;
    let _operation = state.proxy_config_operation().lock().await;
    blocking(app_handle.clone(), window.label().into(), move |state| {
        dns_migration::save(state, &plan_id)
    })
    .await
}
#[tauri::command]
pub async fn gui_dns_migration_status(
    app_handle: AppHandle,
    window: Window,
    plan_id: String,
) -> AppResult<Status> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        dns_migration::status(state, &plan_id)
    })
    .await
}
#[tauri::command]
pub async fn gui_dns_migration_discard(
    app_handle: AppHandle,
    window: Window,
    plan_id: String,
) -> AppResult<()> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    blocking(app_handle.clone(), window.label().into(), move |state| {
        dns_migration::discard(state, &plan_id)
    })
    .await
}
