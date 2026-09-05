use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppConfig;
use crate::models::core_process::CoreProcessState;
use crate::models::kernel_version::KernelInstallResult;
use crate::models::kernel_version::KernelInstallStage;
use crate::services::{
    app_config, common, core_process, file_logger, kernel_manager, network_probe,
    system_proxy_guard,
};
use crate::state::app_state::AppState;

pub(super) async fn apply(
    app: AppHandle,
    state: &AppState,
    prepared: kernel_manager::PreparedKernelInstall,
) -> AppResult<KernelInstallResult> {
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    let was_running = core_process::refresh_status(state)?.state == CoreProcessState::Running;
    let proxy_owned = system_proxy_guard::is_enabled_by_guard()?;
    let tun_enabled = if was_running {
        // Preserve explicit saved intent; for legacy settings, preserve the
        // observed app-owned state. Never guess OFF after a failed query.
        match previous.tun.enabled {
            Some(value) => value,
            None => crate::commands::core_process::app_tun_runtime_enabled(state).await?,
        }
    } else {
        false
    };
    let prepared = Arc::new(prepared);
    kernel_manager::report_install_stage(
        &app,
        &prepared.result.version,
        KernelInstallStage::BackingUp,
    );
    let backup_source = Arc::clone(&prepared);
    let mut transaction = blocking(move || backup_source.backup()).await?;
    if let Err(mut error) = transaction.preserve_config(&previous) {
        if let Err(cancel_error) = transaction.cancel_prepared() {
            error.message.push_str(&format!(
                "; preparation cancellation failed: {}",
                cancel_error.message
            ));
        }
        return Err(error);
    }
    let backup_path = transaction.backup_path().to_owned();

    let result: AppResult<()> = async {
        file_logger::line("kernel upgrade: stopping owned runtime before replacement");
        stop(&app).await?;
        kernel_manager::report_install_stage(
            &app,
            &prepared.result.version,
            KernelInstallStage::Installing,
        );
        let installer = Arc::clone(&prepared);
        blocking(move || installer.install()).await?;
        let mut candidate = common::lock(state.app_config(), "app_config")?.clone();
        candidate.core.executable_path = Some(prepared.result.executable_path.clone());
        app_config::replace(state, candidate)?;
        if was_running {
            kernel_manager::report_install_stage(
                &app,
                &prepared.result.version,
                KernelInstallStage::Starting,
            );
            restore_runtime(&app, state, tun_enabled, proxy_owned, &previous).await?;
        }
        transaction.commit()?;
        Ok(())
    }
    .await;

    if let Err(original) = result {
        kernel_manager::report_install_stage(
            &app,
            &prepared.result.version,
            KernelInstallStage::RollingBack,
        );
        file_logger::line(&format!(
            "kernel upgrade failed; restoring previous installation: {}",
            original.message
        ));
        let mut failures = Vec::new();
        if let Err(error) = stop(&app).await {
            failures.push(format!("stop candidate: {}", error.message));
        }
        // Do not replace an executable whose process could not be stopped.
        if failures.is_empty() {
            if let Err(error) = blocking(move || transaction.rollback()).await {
                failures.push(error.message);
            }
        }
        if let Err(error) = app_config::replace(state, previous.clone()) {
            failures.push(format!("restore configuration: {}", error.message));
        }
        if was_running && failures.is_empty() {
            if let Err(error) =
                restore_runtime(&app, state, tun_enabled, proxy_owned, &previous).await
            {
                failures.push(format!("restore runtime: {}", error.message));
            }
        }
        if !failures.is_empty() {
            if let Err(error) = blocking(system_proxy_guard::disable_with_guard).await {
                failures.push(format!("restore system proxy: {}", error.message));
            }
        }
        let restored = failures.is_empty();
        network_probe::emit_host_network_changed(&app, "core.version_rollback");
        return Err(AppError {
            code: "kernel_upgrade_failed",
            message: if restored {
                format!(
                    "内核升级失败，已恢复升级前的安装和运行状态：{}",
                    original.message
                )
            } else {
                format!(
                    "内核升级失败，自动恢复未完成：{}；{}。备份保留在 {}",
                    original.message,
                    failures.join("；"),
                    backup_path.display()
                )
            },
            details: Some(
                serde_json::json!({"cause":original, "rollbackRestored":restored, "rollbackErrors":failures, "backupPath":backup_path}),
            ),
        });
    }

    file_logger::line("kernel upgrade: installation committed after runtime readiness");
    network_probe::emit_host_network_changed(&app, "core.version_restarted");
    Ok(prepared.result.clone())
}

async fn stop(app: &AppHandle) -> AppResult<()> {
    let app = app.clone();
    blocking(move || {
        let state = app.state::<AppState>();
        core_process::stop_preserving_system_proxy(app.clone(), state).map(|_| ())
    })
    .await
}

async fn restore_runtime(
    app: &AppHandle,
    state: &AppState,
    tun: bool,
    proxy: bool,
    previous: &AppConfig,
) -> AppResult<()> {
    let start_app = app.clone();
    blocking(move || {
        let state = start_app.state::<AppState>();
        core_process::start(start_app.clone(), state).map(|_| ())
    })
    .await?;
    crate::commands::core_process::restore_app_tun_after_core_transition_with_desired(
        state,
        Some(tun),
    )
    .await?;
    if proxy {
        let endpoint = previous.local_proxy.clone();
        blocking(move || {
            crate::services::local_proxy::wait_until_listening(&endpoint.host, endpoint.port)?;
            system_proxy_guard::enable_with_guard_and_bypass(
                &endpoint.host,
                endpoint.port,
                &endpoint.bypass,
            )
        })
        .await?;
    }
    Ok(())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| AppError::internal(format!("kernel upgrade task failed: {error}")))?
}
