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
    plan_id: Option<String>,
) -> AppResult<DocumentReceipt> {
    native_documents::require_host(window.label())?;
    use tauri::Manager;
    let snapshot = app
        .state::<crate::state::app_state::AppState>()
        .client_core_snapshot();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::state::app_state::AppState>();
        state.ensure_client_scope_generation(&snapshot.scope, snapshot.scope_generation.0)?;
        let result = native_documents::export_diagnostics(&app, delivery, plan_id);
        state
            .ensure_client_scope_generation(&snapshot.scope, snapshot.scope_generation.0)
            .map_err(|_| {
                AppError::internal(
                    "交付期间配置或实例代次变化；系统可能已处理原操作，结果未确认，不会自动重试",
                )
            })?;
        result
    })
    .await
    .map_err(|_| AppError::internal("系统导出任务结果未确认，请先检查目标文件"))?
}

#[tauri::command]
pub async fn native_document_preview_diagnostics(
    app: AppHandle,
    window: Window,
) -> AppResult<crate::models::native_documents::DiagnosticPreview> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<crate::state::app_state::AppState>();
        crate::services::diagnostics::preview(state.inner())
    })
    .await
    .map_err(|_| AppError::internal("诊断快照准备失败，未交付文件"))?
}
#[tauri::command]
pub async fn native_diagnostic_capture(
    state: tauri::State<'_, crate::state::app_state::AppState>,
    window: Window,
    request: Option<crate::models::debug::CaptureRequest>,
) -> AppResult<crate::models::debug::CaptureStatus> {
    native_documents::require_host(window.label())?;
    let scope = state.client_core_snapshot().scope;
    crate::models::debug::invalidate_capture_scope(&scope);
    if let Some(request) = request {
        crate::services::native_operation::execute_async(
            state.inner(),
            "diagnostic.capture",
            "ipc-detail",
            std::time::Duration::from_secs(15),
            async { state.commit_diagnostic_capture(&scope, request) },
        )
        .await
    } else {
        Ok(crate::models::debug::capture_status())
    }
}
