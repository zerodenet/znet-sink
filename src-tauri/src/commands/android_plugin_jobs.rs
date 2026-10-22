use crate::errors::{AppError, AppResult};
use tauri_plugin_znet_native_io::{NativeIoExt, PluginJobsSnapshot};
#[tauri::command]
pub async fn mobile_plugin_jobs_status(
    app_handle: tauri::AppHandle,
    window: tauri::Window,
) -> AppResult<PluginJobsSnapshot> {
    crate::services::native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        app_handle
            .native_io()
            .plugin_jobs_status()
            .map_err(|_| AppError {
                code: "unsupported",
                message: "Android 系统后台作业观测暂不可用；请保留任务记录后重新核对".into(),
                details: None,
            })
    })
    .await
    .map_err(|_| AppError::internal("读取系统作业状态未确认"))?
}
