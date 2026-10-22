use crate::{
    errors::{AppError, AppResult},
    models::app_config::AppConfig,
    state::app_state::AppState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSnapshot {
    pub enabled: bool,
    pub expected_revision: String,
    pub permission: &'static str,
    pub reason: Option<&'static str>,
    pub presentation: &'static str,
}

fn revision(config: &AppConfig) -> AppResult<String> {
    serde_json::to_vec(config)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|_| AppError::internal("通知偏好版本读取失败"))
}
fn candidate(current: &AppConfig, expected: &str, enabled: bool) -> AppResult<AppConfig> {
    if revision(current)? != expected {
        return Err(AppError::conflict(
            "notifications",
            "",
            "应用设置已变化，请重新核对通知偏好",
        ));
    }
    let mut next = current.clone();
    next.mobile_notifications_enabled = enabled;
    Ok(next)
}

pub(crate) fn status(app: &AppHandle, request: bool) -> AppResult<NotificationSnapshot> {
    let state = app.state::<AppState>();
    let config = super::common::lock(state.app_config(), "app_config")?.clone();
    let (permission, reason) = platform_permission(app, request);
    Ok(NotificationSnapshot {
        enabled: config.mobile_notifications_enabled,
        expected_revision: revision(&config)?,
        permission,
        reason,
        presentation: "unconfirmed",
    })
}
pub(crate) fn save(
    app: &AppHandle,
    expected: &str,
    enabled: bool,
) -> AppResult<NotificationSnapshot> {
    let state = app.state::<AppState>();
    let current = super::common::lock(state.app_config(), "app_config")?.clone();
    let next = candidate(&current, expected, enabled)?;
    super::app_config::compare_and_replace_local(state.inner(), &current, next)?;
    // Read system state without requesting a permission or posting a notification.
    status(app, false)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn platform_permission(app: &AppHandle, request: bool) -> (&'static str, Option<&'static str>) {
    use tauri_plugin_notification::{NotificationExt, PermissionState};
    let read = || app.notification().permission_state();
    let result = read().and_then(|before| {
        // Upstream Android requestPermissions does not resolve when already
        // granted. A permission is never requested merely by viewing settings.
        if request && before != PermissionState::Granted {
            app.notification().request_permission()?;
        }
        read()
    });
    match result {
        Ok(PermissionState::Granted) => ("granted", None),
        Ok(PermissionState::Denied) => (
            "denied",
            Some("系统拒绝或关闭通知；请在系统设置调整后重新核对。"),
        ),
        Ok(PermissionState::Prompt) => {
            ("prompt", Some("只有点击请求系统授权时才显示系统权限流程。"))
        }
        Ok(PermissionState::PromptWithRationale) => (
            "prompt-with-rationale",
            Some("可选系统通知用于对应来源的状态提示，不代替 VPN 服务必需提示。"),
        ),
        Err(_) => (
            "unknown",
            Some("系统授权读取或请求回执未确认；请重新核对，不自动重复授权。"),
        ),
    }
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn platform_permission(_: &AppHandle, _: bool) -> (&'static str, Option<&'static str>) {
    (
        "unsupported",
        Some("当前为桌面移动布局预览；移动系统权限需原生宿主读取。"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_preferences_never_grant_system_permission() {
        let config: AppConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!config.mobile_notifications_enabled);
    }
    #[test]
    fn only_optional_preference_changes() {
        let current = AppConfig::default();
        let mut expected = current.clone();
        expected.mobile_notifications_enabled = true;
        assert_eq!(
            candidate(&current, &revision(&current).unwrap(), true).unwrap(),
            expected
        );
    }
    #[test]
    fn old_confirmation_cannot_overwrite_changed_settings() {
        let current = AppConfig::default();
        let digest = revision(&current).unwrap();
        let mut changed = current.clone();
        changed.ui.theme = "dark".into();
        assert!(candidate(&changed, &digest, true).is_err());
        assert!(!changed.mobile_notifications_enabled);
    }
}
