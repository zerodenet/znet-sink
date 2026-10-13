use crate::{
    errors::{AppError, AppResult},
    services::{
        mobile_notifications::{self, NotificationSnapshot},
        native_documents,
    },
};
use tauri::{AppHandle, Window};

#[tauri::command]
pub async fn mobile_notification_status(
    app: AppHandle,
    window: Window,
) -> AppResult<NotificationSnapshot> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || mobile_notifications::status(&app, false))
        .await
        .map_err(|_| AppError::internal("系统通知状态读取未确认"))?
}
#[tauri::command]
pub async fn mobile_notification_request(
    app: AppHandle,
    window: Window,
) -> AppResult<NotificationSnapshot> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || mobile_notifications::status(&app, true))
        .await
        .map_err(|_| AppError::internal("系统通知授权结果未确认，请重新核对"))?
}
#[tauri::command]
pub async fn mobile_notification_preference(
    app: AppHandle,
    window: Window,
    expected_revision: String,
    enabled: bool,
) -> AppResult<NotificationSnapshot> {
    native_documents::require_host(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        mobile_notifications::save(&app, &expected_revision, enabled)
    })
    .await
    .map_err(|_| AppError::internal("通知偏好保存结果未确认，请重新核对"))?
}
