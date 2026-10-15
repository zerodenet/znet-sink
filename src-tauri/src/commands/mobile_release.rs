use crate::errors::{AppError, AppResult};
use crate::models::mobile_release::*;
use crate::services::mobile_release;
use crate::state::app_state::AppState;
use tauri::{AppHandle, Emitter, Manager, Window};

fn require_host_label(label: &str) -> AppResult<()> {
    if label != "main" {
        return Err(AppError {
            code: "forbidden",
            message: "发行管理仅允许宿主主窗口调用".into(),
            details: None,
        });
    }
    Ok(())
}
#[tauri::command]
pub async fn mobile_release_catalog(
    app_handle: AppHandle,
    window: Window,
    source: MobileReleaseSource,
    channel: MobileReleaseChannel,
) -> AppResult<MobileReleaseCatalog> {
    require_host_label(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let result = mobile_release::catalog(state.inner(), source, channel);
        let _ = app_handle.emit("mobile-release:schedule-updated", ());
        result
    })
    .await
    .map_err(|_| AppError::internal("发行目录读取未完成"))?
}
#[tauri::command]
pub async fn mobile_release_preferences(
    app_handle: AppHandle,
    window: Window,
) -> AppResult<ReleasePreferencesSnapshot> {
    require_host_label(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        state.mobile_release_scheduler().preferences(state.inner())
    })
    .await
    .map_err(|_| AppError::internal("发行偏好读取未完成"))?
}
#[tauri::command]
pub async fn mobile_release_preferences_save(
    app_handle: AppHandle,
    window: Window,
    expected_revision: String,
    preferences: ReleasePreferences,
    operation_id: Option<String>,
) -> AppResult<ReleasePreferencesSnapshot> {
    require_host_label(window.label())?;
    let id =
        operation_id.ok_or_else(|| AppError::invalid_argument("保存前必须固定发行偏好操作身份"))?;
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let app = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = state.mobile_release_scheduler().save_preferences(
            state.inner(),
            &id,
            &expected_revision,
            preferences,
        );
        let _ = app.emit("mobile-release:schedule-updated", ());
        result
    })
    .await
    .map_err(|_| AppError::internal("发行偏好保存结果未确认；请核对原操作"))?
}
#[tauri::command]
pub async fn mobile_release_schedule(
    app_handle: AppHandle,
    window: Window,
) -> AppResult<ReleaseScheduleSnapshot> {
    require_host_label(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        state.mobile_release_scheduler().snapshot(state.inner())
    })
    .await
    .map_err(|_| AppError::internal("发行检查状态读取未完成"))?
}
#[tauri::command]
pub async fn mobile_release_preferences_operation(
    app_handle: AppHandle,
    window: Window,
    operation_id: String,
) -> AppResult<Option<ReleasePreferenceOperation>> {
    require_host_label(window.label())?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        state
            .mobile_release_scheduler()
            .operation(state.inner(), &operation_id)
    })
    .await
    .map_err(|_| AppError::internal("原发行偏好操作读取未完成"))?
}
#[tauri::command]
pub async fn mobile_release_preferences_confirm(
    app_handle: AppHandle,
    window: Window,
    operation_id: String,
    review_binding: String,
) -> AppResult<ReleasePreferencesSnapshot> {
    require_host_label(window.label())?;
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let app = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = state.mobile_release_scheduler().confirm_preferences(
            state.inner(),
            &operation_id,
            &review_binding,
        );
        let _ = app.emit("mobile-release:schedule-updated", ());
        result
    })
    .await
    .map_err(|_| AppError::internal("原发行偏好确认结果未确认；请只读核对原操作"))?
}
#[cfg(test)]
mod tests {
    #[test]
    fn catalog_and_preferences_remain_host_owned() {
        assert!(super::require_host_label("main").is_ok());
        for label in ["plugin.publisher.component", "traffic-ball", "", "MAIN"] {
            assert_eq!(
                super::require_host_label(label).unwrap_err().code,
                "forbidden"
            );
        }
    }
}
