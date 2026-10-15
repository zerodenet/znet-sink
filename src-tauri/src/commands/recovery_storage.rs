use crate::{
    errors::{AppError, AppResult},
    models::recovery_storage::WorkspaceStorageInspection,
    services::{app_database::inspection, data_dir, native_documents},
    state::app_state::AppState,
};
use tauri::{State, WebviewWindow};

#[tauri::command]
pub async fn gui_workspace_storage_inspect(
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> AppResult<WorkspaceStorageInspection> {
    native_documents::require_host(window.label())?;
    let directory = data_dir()?;
    let session = state.host_session_id().ok().map(str::to_owned);
    tauri::async_runtime::spawn_blocking(move || inspection::inspect(&directory, session))
        .await
        .map_err(|_| AppError::internal("工作区存储检查未能完成；原账本和屏障保留"))
}
