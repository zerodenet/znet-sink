use tauri::State;

use crate::errors::AppResult;
use crate::models::app_config::AppCoreConfig;
use crate::models::kernel_version::{KernelInstallResult, KernelVersionDetect, KernelVersionList};
use crate::services::{common, interaction_mode, kernel_manager};
use crate::state::app_state::AppState;

mod upgrade;

#[tauri::command]
pub async fn kernel_list_versions() -> AppResult<KernelVersionList> {
    // Read-only — available in both lite and pro mode
    tauri::async_runtime::spawn_blocking(kernel_manager::list_available_versions)
        .await
        .map_err(|e| {
            crate::errors::AppError::internal(format!("version list thread panicked: {e}"))
        })?
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
    let _operation = state.proxy_config_operation().lock().await;
    kernel_manager::report_install_stage(
        &app,
        &version,
        crate::models::kernel_version::KernelInstallStage::Preparing,
    );

    let install_app = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        kernel_manager::prepare_version(
            version,
            download_url,
            expected_sha256,
            install_dir,
            install_app,
        )
    })
    .await
    .map_err(|e| crate::errors::AppError::internal(format!("install thread panicked: {e}")))??;

    upgrade::apply(app, state.inner(), outcome).await
}

#[tauri::command]
pub async fn kernel_detect_version(state: State<'_, AppState>) -> AppResult<KernelVersionDetect> {
    // Read-only — available in both lite and pro mode
    let config: AppCoreConfig = { common::lock(state.app_config(), "app_config")?.core.clone() };
    // Process spawn — must run on blocking thread to avoid freezing UI
    tauri::async_runtime::spawn_blocking(move || kernel_manager::detect_installed_version(&config))
        .await
        .map_err(|e| {
            crate::errors::AppError::internal(format!("version detect thread panicked: {e}"))
        })?
}
