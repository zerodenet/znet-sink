use tauri::State;

use crate::errors::AppResult;
use crate::models::kernel_version::{KernelInstallResult, KernelVersionDetect, KernelVersionList};
use crate::services::{common, interaction_mode, kernel_manager};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn kernel_list_versions(state: State<'_, AppState>) -> AppResult<KernelVersionList> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    kernel_manager::list_available_versions()
}

#[tauri::command]
pub async fn kernel_install_version(
    state: State<'_, AppState>,
    version: String,
    download_url: String,
    expected_sha256: Option<String>,
    install_dir: Option<String>,
    app: tauri::AppHandle,
) -> AppResult<KernelInstallResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    tauri::async_runtime::spawn_blocking(move || {
        kernel_manager::install_version(version, download_url, expected_sha256, install_dir, app)
    })
    .await
    .map_err(|e| crate::errors::AppError::internal(format!("install thread panicked: {e}")))?
}

#[tauri::command]
pub fn kernel_detect_version(state: State<'_, AppState>) -> AppResult<KernelVersionDetect> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    let config = common::lock(state.app_config(), "app_config")?;
    kernel_manager::detect_installed_version(&config.core)
}
