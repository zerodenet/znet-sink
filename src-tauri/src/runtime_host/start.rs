use crate::errors::{AppError, AppResult};
use crate::models::core_process::{CoreProcessState, CoreProcessStatus};
use crate::models::logs::{LogLevel, LogSource};
use crate::services::{app_config_store, common::lock, core_config, local_proxy, logs};
use crate::state::app_state::AppState;
use serde_json::json;
use tauri::{AppHandle, Manager, State};

use super::{monitor::spawn_monitor, spawn::spawn_core_child, status::refresh_locked_status};

fn start_inner(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    let monitor_generation = {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        refresh_locked_status(&mut process, state.inner())?;
        if process.child.is_some() {
            return Ok(process.status.clone());
        }
        // Invalidate an older restart loop before export/validation can fail.
        state.next_core_process_monitor_generation()
    };
    // The kernel is a managed service, so it must also be startable before the
    // user imports a proxy profile. In that state we launch a small, persistent
    // control-plane config with no listeners or outbounds. System proxy enable
    // and normal connection flows still require a usable active profile.
    let has_active_profile = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .any(|profile| profile.active);
    if has_active_profile {
        // Exporting the selected profile is the single source of the managed
        // core config path. Invalid active content still fails loudly here.
        core_config::export_active(state.clone())?;
    } else {
        let path = core_config::write_minimal_temp_config()?;
        let config_path = path.to_string_lossy().into_owned();
        {
            let mut app_config = lock(state.app_config(), "app_config")?;
            app_config.core.config_path = Some(config_path.clone());
            app_config_store::save(&app_config_store::default_config_path()?, &app_config)?;
        }
        // `logs::append_entry` reads app_config.logs.max_entries, so the
        // config mutex must be released before recording this lifecycle log.
        let _ = logs::append_entry(
            state.inner(),
            LogSource::App,
            LogLevel::Info,
            "core process start: no active proxy config, using management-only config".to_string(),
            Some(json!({ "configPath": config_path })),
        );
    }

    let config = { lock(state.app_config(), "app_config")?.core.clone() };
    let snapshot = core_config::snapshot_from_config(&config)?;
    if snapshot.config_path.is_none() || snapshot.config_exists != Some(true) {
        return Err(AppError {
            code: "core_config_required",
            message: "当前配置尚未生成可用的内核配置文件，请重新同步或导入配置".to_string(),
            details: Some(json!({
                "configPath": snapshot.config_path,
                "configExists": snapshot.config_exists,
            })),
        });
    }

    if let Err(error) = snapshot.validate_launchable() {
        let message = format!("failed to start core process: {error}");
        let _ = logs::append_entry(
            state.inner(),
            LogSource::App,
            LogLevel::Error,
            message.clone(),
            Some(json!({
                "kernel": snapshot.kernel.clone(),
                "executablePath": snapshot.executable_path.clone(),
                "configPath": snapshot.config_path.clone(),
                "workingDir": snapshot.working_dir.clone(),
                "endpointPath": snapshot.endpoint.path.clone(),
                "warnings": snapshot.warnings.clone(),
                "reason": error,
            })),
        );
        return Err(AppError::invalid_argument(error));
    }

    {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        refresh_locked_status(&mut process, state.inner())?;
        if process.child.is_some() {
            return Ok(process.status.clone());
        }

        // Supersede the previous watchdog before clearing the Stopped exit
        // reason. Starting has a deliberate pre-spawn window (stale-process
        // cleanup and port checks) where there is no managed child. If the
        // old watchdog remains current during that window, it can mistake
        // this intentional transition for a crash and emit/retry spuriously.
        process.status = CoreProcessStatus {
            state: CoreProcessState::Starting,
            pid: None,
            kernel: snapshot.kernel.clone(),
            executable_path: snapshot.executable_path.clone(),
            working_dir: snapshot.working_dir.clone(),
            config_path: snapshot.config_path.clone(),
            endpoint_path: snapshot.endpoint.path.clone(),
            started_at_unix_ms: None,
            exited_at_unix_ms: None,
            exit_code: None,
            exit_reason: None,
            last_error: None,
        };
    }

    // Pre-check the local proxy port before spawning. If something else
    // already occupies it (another proxy, a stale process), the kernel
    // will fail to bind and die immediately — which previously cascaded
    // into a destructive system-proxy disable. Surface the conflict now.
    let endpoint = crate::configuration::preferences::endpoint(state.inner());
    let port_check = endpoint
        .as_ref()
        .map(|(host, port)| local_proxy::check_port_available(host, *port));
    if let Ok(Err(error)) = port_check {
        let (proxy_host, proxy_port) = endpoint.as_ref().unwrap();
        let message = error.message.clone();
        let _ = logs::append_entry(
            state.inner(),
            LogSource::App,
            LogLevel::Error,
            format!("core process: {message}"),
            Some(json!({ "host": proxy_host, "port": proxy_port })),
        );
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        process.status.state = CoreProcessState::Failed;
        process.status.last_error = Some(message);
        process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
        return Err(error);
    }

    let status = spawn_core_child(state.inner(), &snapshot, &app_handle)?;
    // Spawn the watchdog once. It reuses spawn_core_child to restart after
    // crashes, so on restart it must NOT spawn another monitor — that would
    // multiply monitors on every crash.
    spawn_monitor(app_handle.clone(), snapshot, monitor_generation);
    Ok(status)
}

/// Restart the managed kernel while preserving a system proxy owned by this
/// GUI. The local endpoint does not change during an ordinary restart, so
/// restoring and immediately re-enabling the OS proxy would only create two
/// unnecessary macOS authorization prompts.
fn restart_inner(app_handle: AppHandle) -> AppResult<CoreProcessStatus> {
    let state = app_handle.state::<AppState>();
    super::stop::stop_with_proxy_restore(app_handle.clone(), state.clone(), false)?;
    let status = start_inner(app_handle.clone(), state.clone())?;
    crate::services::network_probe::emit_host_network_changed(&app_handle, "core.restarted");
    Ok(status)
}

pub fn start(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    state.runtime_host().run("start", true, || {
        if state.is_shutting_down() {
            return Err(AppError::internal("application is shutting down"));
        }
        start_inner(app_handle.clone(), state.clone())
    })
}
pub fn restart(app_handle: AppHandle) -> AppResult<CoreProcessStatus> {
    let state = app_handle.state::<AppState>();
    state.runtime_host().run("restart", true, || {
        if state.is_shutting_down() {
            return Err(AppError::internal("application is shutting down"));
        }
        restart_inner(app_handle.clone())
    })
}
