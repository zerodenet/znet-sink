use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::models::core_process::CoreProcessState;
use crate::services::common::lock;
use crate::services::{app_config, core_process};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn app_config_get(state: State<'_, AppState>) -> AppResult<AppConfig> {
    app_config::get(state)
}

#[tauri::command]
pub fn app_config_update(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    patch: AppConfigPatch,
) -> AppResult<AppConfig> {
    // Snapshot the old config before applying changes.
    let old_config = app_config::get(state.clone())?;

    // Apply the patch and persist.
    let new_config = app_config::update(state.clone(), patch)?;

    // Detect whether a restart-worthy field changed while the kernel is
    // running.  We check the kernel state *after* the config update so the
    // lock is released — `core_process::stop/start` acquire their own locks.
    let kernel_running = {
        let process = lock(state.core_process(), "core_process")?;
        process.status.state == CoreProcessState::Running
    };

    if kernel_running {
        let needs_restart = old_config.core.executable_path != new_config.core.executable_path
            || old_config.core.socket != new_config.core.socket
            || old_config.core.working_dir != new_config.core.working_dir;

        if needs_restart {
            let _ = core_process::stop(state.clone());
            // Drop the stale multiplexed connection so the next request
            // opens a fresh one against the (possibly new) endpoint.
            crate::kernel::connection::reset();

            if new_config.core.auto_start {
                let _ = core_process::start(app_handle, state);
            }
        }
    }

    Ok(new_config)
}
