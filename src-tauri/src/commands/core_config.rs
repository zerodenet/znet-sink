use tauri::State;

use crate::errors::AppResult;
use crate::models::core_config::{CoreConfigExportResult, CoreDownloadResult, CoreKernelInfo};
use crate::services::{core_config, interaction_mode};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn core_config_get(state: State<'_, AppState>) -> AppResult<CoreKernelInfo> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    core_config::inspect(state)
}

#[tauri::command]
pub fn core_config_export_active(state: State<'_, AppState>) -> AppResult<CoreConfigExportResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    core_config::export_active(state)
}

#[tauri::command]
pub async fn core_download_latest(
    state: State<'_, AppState>,
    install_dir: Option<String>,
) -> AppResult<CoreDownloadResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    tauri::async_runtime::spawn_blocking(move || core_config::download_latest(install_dir))
        .await
        .map_err(|e| crate::errors::AppError::internal(format!("download thread panicked: {e}")))?
}
