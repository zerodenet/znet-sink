use crate::{
    errors::{AppError, AppResult},
    models::native_documents::{
        DocumentCapabilities, DocumentDelivery, DocumentPurpose, DocumentReceipt,
    },
    services::native_documents,
};
use tauri::{AppHandle, Window};

#[tauri::command]
pub fn native_document_capabilities(
    app: AppHandle,
    window: Window,
) -> AppResult<DocumentCapabilities> {
    native_documents::require_host(window.label())?;
    Ok(native_documents::capabilities(&app))
}
#[tauri::command]
pub async fn native_document_pick(
    app: AppHandle,
    window: Window,
    purpose: DocumentPurpose,
) -> AppResult<DocumentReceipt> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || native_documents::pick(&app, purpose))
        .await
        .map_err(|_| AppError::internal("系统文档读取任务未完成；原草稿保留"))?
}
#[tauri::command]
pub async fn native_document_export_profile(
    app: AppHandle,
    window: Window,
    id: String,
    expected_updated_at: u64,
    delivery: DocumentDelivery,
    credentials_acknowledged: bool,
) -> AppResult<DocumentReceipt> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        native_documents::export_profile(
            &app,
            &id,
            expected_updated_at,
            delivery,
            credentials_acknowledged,
        )
    })
    .await
    .map_err(|_| AppError::internal("系统导出任务结果未确认，请先检查目标文件"))?
}
#[tauri::command]
pub async fn native_document_export_settings(
    app: AppHandle,
    window: Window,
    expected_revision: String,
    delivery: DocumentDelivery,
) -> AppResult<DocumentReceipt> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        native_documents::export_settings(&app, &expected_revision, delivery)
    })
    .await
    .map_err(|_| AppError::internal("系统导出任务结果未确认，请先检查目标文件"))?
}
#[tauri::command]
pub async fn native_document_export_diagnostics(
    app: AppHandle,
    window: Window,
    delivery: DocumentDelivery,
) -> AppResult<DocumentReceipt> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        native_documents::export_diagnostics(&app, delivery)
    })
    .await
    .map_err(|_| AppError::internal("系统导出任务结果未确认，请先检查目标文件"))?
}
