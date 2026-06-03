use tauri::State;

use crate::errors::AppResult;
use crate::models::app_config::AppCoreConfig;
use crate::models::kernel_version::{KernelInstallResult, KernelVersionDetect, KernelVersionList};
use crate::services::{common, interaction_mode, kernel_manager};
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn kernel_list_versions(state: State<'_, AppState>) -> AppResult<KernelVersionList> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    // Network I/O — must run on blocking thread to avoid freezing UI
    tauri::async_runtime::spawn_blocking(|| kernel_manager::list_available_versions())
        .await
        .map_err(|e| crate::errors::AppError::internal(format!("version list thread panicked: {e}")))?
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
pub async fn kernel_detect_version(state: State<'_, AppState>) -> AppResult<KernelVersionDetect> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    let config: AppCoreConfig = {
        common::lock(state.app_config(), "app_config")?.core.clone()
    };
    // Process spawn — must run on blocking thread to avoid freezing UI
    tauri::async_runtime::spawn_blocking(move || kernel_manager::detect_installed_version(&config))
        .await
        .map_err(|e| crate::errors::AppError::internal(format!("version detect thread panicked: {e}")))?
}
