use crate::{
    errors::{AppError, AppResult},
    models::mobile_release::MobileReleaseChannel,
    services::android_app_update,
};
use tauri::{AppHandle, Window};
use tauri_plugin_znet_native_io::AppUpdateSnapshot;

fn host(window: &Window) -> AppResult<()> {
    if window.label() != "main" {
        return Err(AppError {
            code: "forbidden",
            message: "应用安装仅允许宿主主窗口调用".into(),
            details: None,
        });
    }
    Ok(())
}
#[tauri::command]
pub async fn mobile_app_update_status(
    app_handle: AppHandle,
    window: Window,
) -> AppResult<AppUpdateSnapshot> {
    host(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        android_app_update::read(&app_handle, "status", None)
    })
    .await
    .map_err(|_| AppError::internal("安装状态读取未完成"))?
}
#[tauri::command]
pub async fn mobile_app_update_download(
    app_handle: AppHandle,
    window: Window,
    channel: MobileReleaseChannel,
    tag: String,
    asset_id: String,
    expected_sha256: String,
    operation_id: String,
) -> AppResult<AppUpdateSnapshot> {
    host(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        android_app_update::download(
            &app_handle,
            channel,
            &tag,
            &asset_id,
            &expected_sha256,
            operation_id,
        )
    })
    .await
    .map_err(|_| AppError::internal("下载请求结果未确认；请核对原操作"))?
}
#[tauri::command]
pub async fn mobile_app_update_action(
    app_handle: AppHandle,
    window: Window,
    action: String,
    operation_id: Option<String>,
) -> AppResult<AppUpdateSnapshot> {
    host(&window)?;
    if !matches!(
        action.as_str(),
        "install" | "confirm" | "cancel" | "settings" | "store"
    ) {
        return Err(AppError::invalid_argument("不允许的安装动作"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        android_app_update::read(&app_handle, &action, operation_id)
    })
    .await
    .map_err(|_| AppError::internal("安装请求结果未确认；请核对原操作"))?
}
