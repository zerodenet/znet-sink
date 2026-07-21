use tauri::{Manager, State};

use crate::errors::AppResult;
use crate::models::app_config::AppCoreConfig;
use crate::models::kernel_version::{KernelInstallResult, KernelVersionDetect, KernelVersionList};
use crate::services::{
    app_config, common, core_process, interaction_mode, kernel_manager, system_proxy_guard,
};
use crate::state::app_state::AppState;

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

    let install_app = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        kernel_manager::install_version(
            version,
            download_url,
            expected_sha256,
            install_dir,
            install_app,
        )
    })
    .await
    .map_err(|e| crate::errors::AppError::internal(format!("install thread panicked: {e}")))??;

    // Persist the new executable path so subsequent starts and version
    // detection pick up the freshly installed binary.
    let executable_path = outcome.result.executable_path.clone();
    let mut next_config = common::lock(state.app_config(), "app_config")?.clone();
    next_config.core.executable_path = Some(executable_path);
    app_config::replace(state.inner(), next_config)?;

    if outcome.restart_core {
        let restart_app = app.clone();
        let restore_system_proxy = outcome.restore_system_proxy;
        tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
            let restart_state = restart_app.state::<AppState>();
            let proxy_endpoint = if restore_system_proxy {
                let config = common::lock(restart_state.app_config(), "app_config")?;
                Some((config.local_proxy.host.clone(), config.local_proxy.port))
            } else {
                None
            };

            core_process::start(restart_app.clone(), restart_state)?;
            if let Some((host, port)) = proxy_endpoint {
                system_proxy_guard::enable_with_guard(&host, port)?;
            }
            Ok(())
        })
        .await
        .map_err(|error| {
            crate::errors::AppError::internal(format!("kernel restart task panicked: {error}"))
        })?
        .map_err(|error| {
            crate::errors::AppError::internal(format!(
                "kernel installed but failed to restart: {}",
                error.message
            ))
        })?;
    }

    Ok(outcome.result)
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
