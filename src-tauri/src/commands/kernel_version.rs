use tauri::State;

use crate::errors::AppResult;
use crate::models::{app_config::AppCoreConfig, logs::LogSource, logs::LogLevel};
use crate::models::kernel_version::{KernelInstallResult, KernelVersionDetect, KernelVersionList};
use crate::services::{app_config_store, common, core_process, interaction_mode, kernel_manager, logs};
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn kernel_list_versions(_state: State<'_, AppState>) -> AppResult<KernelVersionList> {
    // Read-only — available in both lite and pro mode
    tauri::async_runtime::spawn_blocking(|| kernel_manager::list_available_versions())
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

    // Stop the running core so the old binary isn't locked during
    // extraction.  On Windows a running .exe cannot be overwritten.
    let _ = core_process::stop(state.clone());
    let _ = logs::append_entry(
        state.inner(),
        LogSource::App,
        LogLevel::Info,
        format!("kernel upgrade: stopped core before installing v{version}"),
        None,
    );

    let result = tauri::async_runtime::spawn_blocking(move || {
        kernel_manager::install_version(version, download_url, expected_sha256, install_dir, app)
    })
    .await
    .map_err(|e| crate::errors::AppError::internal(format!("install thread panicked: {e}")))??;

    // Persist the new executable path so subsequent starts and version
    // detection pick up the freshly installed binary.
    let executable_path = result.executable_path.clone();
    {
        let mut app_config = common::lock(state.app_config(), "app_config")?;
        app_config.core.executable_path = Some(executable_path);
        let snapshot = app_config.clone();
        drop(app_config);
        app_config_store::save(&app_config_store::default_config_path()?, &snapshot)?;
    }

    Ok(result)
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
