use crate::configuration::local_edits::{self, Edits};
use crate::errors::{AppError, AppResult};
use crate::services::common::lock;
use crate::state::app_state::AppState;
use serde_json::Value;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn profile_settings_get(state: State<'_, AppState>) -> AppResult<Value> {
    local_edits::view(state.inner())
}

#[tauri::command]
pub async fn profile_settings_apply(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    changes: Edits,
    reset: Vec<String>,
) -> AppResult<Value> {
    let _operation = state.proxy_config_operation().lock().await;
    let (active, _) = local_edits::active(state.inner())?;
    if active != profile_id {
        return Err(AppError::conflict(
            "configuration",
            "profile",
            "当前配置已切换，请重新读取后再修改",
        ));
    }
    let old = lock(state.app_config(), "app_config")?.clone();
    let next = local_edits::candidate(&old, &active, changes, &reset)?;
    if old != next {
        super::app_config::apply_candidate(app_handle, state.clone(), old, next).await?;
    }
    let mut result = local_edits::view(state.inner())?;
    result["applied"] = serde_json::json!(
        crate::services::core_process::refresh_status(state.inner())?.state
            == crate::models::core_process::CoreProcessState::Running
    );
    Ok(result)
}
