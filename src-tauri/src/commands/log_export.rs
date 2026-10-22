use crate::{
    errors::{AppError, AppResult},
    services::{
        log_export::{self, LogDelivery, LogExportPreview, LogExportReceipt, LogExportRequest},
        native_documents, native_operation,
    },
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, Window};
#[tauri::command]
pub async fn log_export_preview(
    app: AppHandle,
    window: Window,
    request: LogExportRequest,
) -> AppResult<LogExportPreview> {
    native_documents::require_host(window.label())?;
    let state = app.state::<AppState>();
    let worker = app.clone();
    native_operation::execute_async(
        state.inner(),
        "diagnostic.export",
        "log-selection",
        std::time::Duration::from_secs(30),
        async move {
            tauri::async_runtime::spawn_blocking(move || {
                let state = worker.state::<AppState>();
                log_export::preview(state.inner(), request)
            })
            .await
            .map_err(|_| AppError::internal("日志脱敏预览未完成，原列表保留"))?
        },
    )
    .await
}
#[tauri::command]
pub async fn log_export_deliver(
    app: AppHandle,
    window: Window,
    plan_id: String,
    delivery: LogDelivery,
) -> AppResult<LogExportReceipt> {
    native_documents::require_host(window.label())?;
    let state = app.state::<AppState>();
    let worker = app.clone();
    native_operation::execute_async(
        state.inner(),
        "diagnostic.export",
        "log-selection",
        std::time::Duration::from_secs(30),
        async move {
            tauri::async_runtime::spawn_blocking(move || {
                log_export::deliver(&worker, &plan_id, delivery)
            })
            .await
            .map_err(|_| AppError::internal("日志交付结果未确认；不自动重试，原列表保留"))?
        },
    )
    .await
}

#[tauri::command]
pub fn log_export_discard(
    state: tauri::State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<()> {
    native_documents::require_host(window.label())?;
    crate::services::diagnostics::discard_selected(state.inner(), &plan_id)
}
