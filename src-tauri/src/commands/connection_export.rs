use crate::{
    errors::AppResult,
    models::{
        debug::DebugFrameQuery,
        native_documents::{DocumentDelivery, DocumentReceipt},
    },
    services::{connection_export, native_documents},
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, WebviewWindow};
fn admit(app: &AppHandle, label: &str) -> AppResult<()> {
    native_documents::require_host(label)?;
    if !native_documents::capabilities(app).available {
        return Err(crate::errors::AppError::invalid_argument(
            "当前平台没有原生文档适配器",
        ));
    }
    Ok(())
}
#[tauri::command]
pub async fn native_connection_export_preview(
    app: AppHandle,
    window: WebviewWindow,
    query: DebugFrameQuery,
) -> AppResult<connection_export::ConnectionExportPreview> {
    admit(&app, window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        connection_export::preview(app.state::<AppState>().inner(), query)
    })
    .await
    .map_err(|_| crate::errors::AppError::internal("连接快照读取失败"))?
}
#[tauri::command]
pub async fn native_connection_export_deliver(
    app: AppHandle,
    window: WebviewWindow,
    plan_id: String,
    delivery: DocumentDelivery,
) -> AppResult<DocumentReceipt> {
    admit(&app, window.label())?;
    connection_export::deliver(&app, &plan_id, delivery).await
}
#[tauri::command]
pub async fn native_active_connection_export_preview(
    app: AppHandle,
    window: WebviewWindow,
    query: connection_export::ActiveConnectionExportFilter,
) -> AppResult<connection_export::ConnectionExportPreview> {
    admit(&app, window.label())?;
    connection_export::preview_active(app.state::<AppState>().inner(), query).await
}
