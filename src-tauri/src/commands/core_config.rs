use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::core_config::{CoreConfigExportResult, CoreKernelInfo};
use crate::services::{core_config, interaction_mode};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn core_config_get(state: State<'_, AppState>) -> AppResult<CoreKernelInfo> {
    // Read-only kernel inspection — available in both lite and pro mode
    core_config::inspect(state)
}

#[tauri::command]
pub fn core_config_export_active(state: State<'_, AppState>) -> AppResult<CoreConfigExportResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    core_config::export_active(state)
}

/// Compatibility tombstone for builds/frontends that still know the old
/// command name. The duplicate downloader has been removed; all kernel
/// installation must go through kernel_install_version / kernel_manager.
#[tauri::command]
pub fn core_download_latest(
    state: State<'_, AppState>,
    _install_dir: Option<String>,
) -> AppResult<()> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    Err(AppError::invalid_argument(
        "core_download_latest has been removed; use kernel version management",
    ))
}
