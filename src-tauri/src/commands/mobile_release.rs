use crate::errors::{AppError, AppResult};
use crate::models::mobile_release::*;
use crate::services::{common, mobile_release};
use crate::state::app_state::AppState;
use tauri::{State, Window};

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
    state: State<'_, AppState>,
    window: Window,
    source: MobileReleaseSource,
    channel: MobileReleaseChannel,
) -> AppResult<MobileReleaseCatalog> {
    require_host_label(window.label())?;
    let manager = state.capabilities().clone();
    tauri::async_runtime::spawn_blocking(move || mobile_release::catalog(&manager, source, channel))
        .await
        .map_err(|_| AppError::internal("发行目录读取未完成"))?
}

#[tauri::command]
pub async fn mobile_release_preferences(
    state: State<'_, AppState>,
    window: Window,
) -> AppResult<ReleasePreferencesSnapshot> {
    require_host_label(window.label())?;
    let _operation = state.proxy_config_operation().lock().await;
    let current = common::lock(state.app_config(), "app_config")?;
    mobile_release::preferences(&current)
}

#[tauri::command]
pub async fn mobile_release_preferences_save(
    state: State<'_, AppState>,
    window: Window,
    expected_revision: String,
    preferences: ReleasePreferences,
) -> AppResult<ReleasePreferencesSnapshot> {
    require_host_label(window.label())?;
    // These ordinary app preferences do not grant runtime or installer permissions.
    let _operation = state.proxy_config_operation().lock().await;
    mobile_release::save_preferences(state.inner(), &expected_revision, preferences)
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
