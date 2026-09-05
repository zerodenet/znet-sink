use tauri::{AppHandle, State};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::models::core_process::CoreProcessState;
use crate::services::kernel_settings::{self, KernelSettingsExportResult};
use crate::services::{app_config, core_process, rule_overlay, system_proxy_guard};
use crate::state::app_state::AppState;

mod effects;
pub(crate) mod tun_settings;

#[tauri::command]
pub async fn app_config_apply_tun(
    state: State<'_, AppState>,
    tun: crate::models::app_config::AppTunConfigPatch,
) -> AppResult<AppConfig> {
    let _operation = state.proxy_config_operation().lock().await;
    tun_settings::apply(state.inner(), tun).await
}

#[tauri::command]
pub fn app_config_get(state: State<'_, AppState>) -> AppResult<AppConfig> {
    app_config::get(state)
}

#[tauri::command]
pub fn app_config_export_kernel_settings(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<KernelSettingsExportResult> {
    let config = app_config::get(state)?;
    kernel_settings::export_to_path(&config, path)
}

async fn restart_core_and_restore_tun(
    app_handle: AppHandle,
    state: &AppState,
    tun_desired_override: Option<bool>,
) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || core_process::restart(app_handle).map(|_| ()))
        .await
        .map_err(|error| AppError::internal(format!("core transition task failed: {error}")))??;
    crate::commands::core_process::restore_app_tun_after_core_transition_with_desired(
        state,
        tun_desired_override,
    )
    .await
}

#[tauri::command]
pub async fn app_config_import_kernel_settings(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<AppConfig> {
    let _operation = state.proxy_config_operation().lock().await;
    let old_config = app_config::get(state.clone())?;
    let new_config = kernel_settings::import_from_path(&old_config, path)?;
    if new_config == old_config {
        return Ok(new_config);
    }
    rule_overlay::validate_app_config_candidate(state.inner(), &new_config)?;

    let kernel_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let legacy_tun_runtime_enabled = if kernel_running
        && (old_config.tun.enabled.is_none() || new_config.tun.enabled.is_none())
    {
        Some(crate::commands::core_process::app_tun_runtime_enabled(state.inner()).await?)
    } else {
        None
    };
    let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
    app_config::replace(state.inner(), new_config.clone())?;

    if kernel_running {
        let imported_tun_override = new_config
            .tun
            .enabled
            .is_none()
            .then_some(legacy_tun_runtime_enabled.unwrap_or(false));
        let transition =
            restart_core_and_restore_tun(app_handle.clone(), state.inner(), imported_tun_override)
                .await;

        if let Err(error) = transition {
            let storage_rollback = app_config::replace(state.inner(), old_config.clone())
                .err()
                .map(|rollback| rollback.message);
            crate::kernel::connection::reset();
            let rollback_tun_override = old_config
                .tun
                .enabled
                .is_none()
                .then_some(legacy_tun_runtime_enabled.unwrap_or(false));
            let mut runtime_rollback = restart_core_and_restore_tun(
                app_handle.clone(),
                state.inner(),
                rollback_tun_override,
            )
            .await
            .err()
            .map(|rollback| rollback.message);
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

            let mut message = format!(
                "failed to apply imported client kernel settings: {}",
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

    Ok(new_config)
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

    // Read legacy capture intent before replacing settings or stopping the process.
    let was_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let new_config = app_config::prepare_update(&old_config, patch)?;
    let effects = effects::between(&old_config, &new_config);
    let legacy_tun = if was_running && effects.restart && old_config.tun.enabled.is_none() {
        Some(crate::commands::core_process::app_tun_runtime_enabled(state.inner()).await?)
    } else {
        None
    };
    if was_running && old_config.dns != new_config.dns {
        return Err(AppError::invalid_argument(
            "运行中 DNS 配置必须通过 DNS 应用事务修改，请使用 DNS 设置页的应用操作",
        ));
    }
    let custom_endpoint = old_config.local_proxy.source_proxy_config_id.is_some();
    if custom_endpoint
        && (old_config.local_proxy.host != new_config.local_proxy.host
            || old_config.local_proxy.port != new_config.local_proxy.port)
    {
        return Err(AppError::invalid_argument(
            "当前代理入口由配置文件定义，请在配置编辑器修改入站地址和端口",
        ));
    }
    if effects.restart || effects.recompose || old_config.dns != new_config.dns {
        rule_overlay::validate_app_config_candidate(state.inner(), &new_config)?;
    }
    app_config::replace(state.inner(), new_config.clone())?;

    // Process transitions run without holding the settings mutex.
    let kernel_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
    let runtime_config_changed = effects.recompose;

    if kernel_running {
        if effects.restart {
            // Drop the stale multiplexed connection so the next request
            // opens a fresh one against the (possibly new) endpoint.
            crate::kernel::connection::reset();

            // auto_start is a next-launch preference, not permission to stop
            // a kernel the user already started manually.
            let transition =
                restart_core_and_restore_tun(app_handle.clone(), state.inner(), legacy_tun).await;

            if let Err(error) = transition {
                let storage_rollback = app_config::replace(state.inner(), old_config.clone())
                    .err()
                    .map(|rollback| rollback.message);
                crate::kernel::connection::reset();
                let mut runtime_rollback =
                    restart_core_and_restore_tun(app_handle.clone(), state.inner(), legacy_tun)
                        .await
                        .err()
                        .map(|error| error.message);
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
        } else if runtime_config_changed {
            // Endpoint, routing and URLTest preferences change the effective
            // runtime configuration without rewriting subscription sources.
            if let Err(error) =
                rule_overlay::reconcile_current_config_locked(app_handle.clone()).await
            {
                let storage_rollback = app_config::replace(state.inner(), old_config.clone())
                    .err()
                    .map(|rollback| rollback.message);
                let runtime_rollback = if error.is_unavailable()
                    || matches!(error.code, "conflict" | "config_apply_uncertain")
                {
                    Some(
                        "configuration completion is uncertain; no competing apply was submitted"
                            .to_owned(),
                    )
                } else {
                    rule_overlay::reconcile_current_config_locked(app_handle.clone())
                        .await
                        .err()
                        .map(|rollback| rollback.message)
                };

                let mut message = format!(
                    "failed to apply effective client configuration: {}",
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

    if managed_proxy_enabled && effects.retarget_proxy {
        if let Err(error) = system_proxy_guard::enable_with_guard_and_bypass(
            &new_config.local_proxy.host,
            new_config.local_proxy.port,
            &new_config.local_proxy.bypass,
        ) {
            let mut error = error;
            let storage = app_config::replace(state.inner(), old_config.clone());
            if let Err(rollback) = storage {
                error
                    .message
                    .push_str(&format!("; settings rollback failed: {}", rollback.message));
            } else if kernel_running && (runtime_config_changed || effects.restart) {
                let rollback = if effects.restart {
                    restart_core_and_restore_tun(app_handle.clone(), state.inner(), legacy_tun)
                        .await
                } else {
                    rule_overlay::reconcile_current_config_locked(app_handle.clone()).await
                };
                if let Err(rollback) = rollback {
                    error
                        .message
                        .push_str(&format!("; runtime rollback failed: {}", rollback.message));
                }
            }
            if let Err(rollback) = system_proxy_guard::enable_with_guard_and_bypass(
                &old_config.local_proxy.host,
                old_config.local_proxy.port,
                &old_config.local_proxy.bypass,
            ) {
                error.message.push_str(&format!(
                    "; system proxy rollback failed: {}",
                    rollback.message
                ));
            }
            return Err(error);
        }
    }

    Ok(new_config)
}
