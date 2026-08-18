use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::zero;
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::models::core_process::CoreProcessState;
use crate::services::{
    app_config, core_config, core_process, rule_overlay, system_proxy_guard,
};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn app_config_get(state: State<'_, AppState>) -> AppResult<AppConfig> {
    app_config::get(state)
}

#[tauri::command]
pub async fn app_config_update(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    patch: AppConfigPatch,
) -> AppResult<AppConfig> {
    let _operation = state.proxy_config_operation().lock().await;
    // Snapshot the old config before applying changes.
    let old_config = app_config::get(state.clone())?;

    // Apply the patch and persist.
    let new_config = app_config::update(state.clone(), patch)?;

    // Detect whether a restart-worthy field changed while the kernel is
    // running. We check the kernel state *after* the config update so the
    // lock is released — `core_process::stop/start` acquire their own locks.
    let kernel_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
    let url_test_tolerance_changed =
        old_config.url_test.tolerance_ms != new_config.url_test.tolerance_ms;
    let tun_defaults_changed = old_config.tun != new_config.tun;

    if kernel_running {
        let needs_restart = old_config.core.executable_path != new_config.core.executable_path
            || old_config.core.socket != new_config.core.socket
            || old_config.core.working_dir != new_config.core.working_dir
            || old_config.core.config_path != new_config.core.config_path;

        // Declarative TUN still needs the same capability-gated Windows
        // compatibility preparation as the legacy command surface. Do this
        // before config.apply, but never create a competing command-managed
        // TUN instance.
        if tun_defaults_changed && new_config.tun.enabled == Some(true) && !needs_restart {
            let opts = core_config::ipc_options_from_app_config(&new_config.core);
            let prepared = zero::runtime::prepare_tun_enable(Some(opts)).await;
            match prepared {
                Ok(status) if status.supported => {}
                Ok(_) => {
                    let _ = app_config::replace(state.inner(), old_config.clone());
                    return Err(AppError::invalid_argument(
                        "the current Zero runtime does not support TUN",
                    ));
                }
                Err(error) => {
                    let storage_rollback = app_config::replace(state.inner(), old_config.clone())
                        .err()
                        .map(|rollback| rollback.message);
                    let mut message = format!(
                        "failed to prepare TUN runtime configuration: {}",
                        error.message
                    );
                    if let Some(storage_rollback) = storage_rollback {
                        message.push_str(&format!(
                            "; configuration rollback failed: {storage_rollback}"
                        ));
                    }
                    return Err(AppError::internal(message));
                }
            }
        }

        if needs_restart {
            // Drop the stale multiplexed connection so the next request
            // opens a fresh one against the (possibly new) endpoint.
            crate::kernel::connection::reset();

            let transition_app = app_handle.clone();
            let should_start = new_config.core.auto_start;
            let transition = tauri::async_runtime::spawn_blocking(move || {
                if should_start {
                    core_process::restart(transition_app).map(|_| ())
                } else {
                    let transition_state = transition_app.state::<AppState>();
                    core_process::stop(transition_app.clone(), transition_state).map(|_| ())
                }
            })
            .await
            .map_err(|error| AppError::internal(format!("core transition task failed: {error}")))
            .and_then(|result| result);

            if let Err(error) = transition {
                let storage_rollback = app_config::replace(state.inner(), old_config.clone())
                    .err()
                    .map(|rollback| rollback.message);
                crate::kernel::connection::reset();
                let rollback_app = app_handle.clone();
                let mut runtime_rollback = tauri::async_runtime::spawn_blocking(move || {
                    core_process::restart(rollback_app).map(|_| ())
                })
                .await
                .map_err(|join| join.to_string())
                .and_then(|result| result.map_err(|rollback| rollback.message))
                .err();
                if managed_proxy_enabled
                    && runtime_rollback.is_none()
                    && !system_proxy_guard::is_enabled_by_guard().unwrap_or(false)
                {
                    if let Err(proxy_error) = system_proxy_guard::enable_with_guard_and_bypass(
                        &old_config.local_proxy.host,
                        old_config.local_proxy.port,
                        &old_config.local_proxy.bypass,
                    ) {
                        runtime_rollback = Some(format!(
                            "system proxy rollback failed: {}",
                            proxy_error.message
                        ));
                    }
                }

                let mut message = format!("failed to apply core configuration: {}", error.message);
                if let Some(storage_rollback) = storage_rollback {
                    message.push_str(&format!(
                        "; configuration rollback failed: {storage_rollback}"
                    ));
                }
                if let Some(runtime_rollback) = runtime_rollback {
                    message.push_str(&format!("; runtime rollback failed: {runtime_rollback}"));
                }
                return Err(AppError::internal(message));
            }
        } else if url_test_tolerance_changed || tun_defaults_changed {
            // URLTest and TUN are client-owned effective-config preferences.
            // The stored subscription/profile remains untouched; recomposition
            // injects defaults only where the source profile omitted them.
            if let Err(error) =
                rule_overlay::reconcile_current_config_locked(app_handle.clone()).await
            {
                let storage_rollback = app_config::replace(state.inner(), old_config.clone())
                    .err()
                    .map(|rollback| rollback.message);
                let runtime_rollback = rule_overlay::reconcile_current_config_locked(
                    app_handle.clone(),
                )
                .await
                .err()
                .map(|rollback| rollback.message);

                let mut message = format!(
                    "failed to apply effective runtime configuration: {}",
                    error.message
                );
                if let Some(storage_rollback) = storage_rollback {
                    message.push_str(&format!(
                        "; configuration rollback failed: {storage_rollback}"
                    ));
                }
                if let Some(runtime_rollback) = runtime_rollback {
                    message.push_str(&format!("; runtime rollback failed: {runtime_rollback}"));
                }
                return Err(AppError::internal(message));
            }
        }
    }

    if managed_proxy_enabled && old_config.local_proxy.bypass != new_config.local_proxy.bypass {
        system_proxy_guard::disable_with_guard()?;
        if let Err(error) = system_proxy_guard::enable_with_guard_and_bypass(
            &new_config.local_proxy.host,
            new_config.local_proxy.port,
            &new_config.local_proxy.bypass,
        ) {
            let _ = system_proxy_guard::enable_with_guard_and_bypass(
                &old_config.local_proxy.host,
                old_config.local_proxy.port,
                &old_config.local_proxy.bypass,
            );
            let _ = app_config::replace(state.inner(), old_config.clone());
            if kernel_running && (url_test_tolerance_changed || tun_defaults_changed) {
                let _ = rule_overlay::reconcile_current_config_locked(app_handle.clone()).await;
            }
            return Err(error);
        }
    }

    Ok(new_config)
}
