use crate::errors::{AppError, AppResult};
use crate::models::{
    core_config::CoreConfigSnapshot,
    core_process::{CoreProcessExitReason, CoreProcessState, CoreProcessStatus},
    logs::{LogLevel, LogSource},
};
use crate::services::{common, common::lock, logs};
use crate::state::app_state::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use super::{logging::parse_kernel_log_line, readiness};
use crate::services::common::strip_ansi;
use std::io::{BufRead, BufReader};
use std::process::Stdio;

/// Spawn the kernel child process, wire up its stderr pump, and record the
/// running status. Shared between the initial [`start`] and the watchdog's
/// crash-restart path so neither duplicates the other's bookkeeping.
///
/// Does NOT spawn the watchdog monitor — [`start`] does that exactly once,
/// and the monitor reuses this function to restart without spawning another
/// monitor (which would otherwise multiply on every crash).
pub(super) fn spawn_core_child(
    state: &AppState,
    snapshot: &CoreConfigSnapshot,
    app_handle: &AppHandle,
) -> AppResult<CoreProcessStatus> {
    lock(state.runtime_host().process(), "core_process")?.ensure_spawn_allowed()?;
    let tun = lock(state.app_config(), "app_config")?.tun.clone();
    if let Err(error) =
        crate::capture::residue::confirm_absent(&tun, snapshot.working_dir.as_deref())
    {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        process.status.state = CoreProcessState::Failed;
        process.status.last_error = Some(error.message.clone());
        return Err(error);
    }
    let executable_path = snapshot.executable_path.as_deref().unwrap_or_default();

    if let Err(error) = crate::services::kernel_manager::transaction::ensure_no_interrupted_upgrade(
        &crate::services::data_dir()?.join("kernel-rollback"),
    ) {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        process.status.state = CoreProcessState::Failed;
        process.status.last_error = Some(error.message.clone());
        process.status.exited_at_unix_ms = Some(common::now_unix_ms());
        return Err(error);
    }

    let _ = logs::append_entry(
        state,
        LogSource::App,
        LogLevel::Info,
        format!(
            "core process: spawning {} with args {:?}",
            executable_path, snapshot.launch_args
        ),
        None,
    );

    let mut command = crate::services::kernel_command::command(executable_path);
    command.args(&snapshot.launch_args);
    if let Some(working_dir) = snapshot.working_dir.as_deref() {
        command.current_dir(working_dir);
    }
    // Keep the write end in `Child::stdin` for the entire managed lifetime.
    // If the GUI exits normally, crashes, or is force-killed, every desktop
    // OS closes this inherited pipe and Zero observes EOF.
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());

    let mut child = match super::child::spawn(&mut command) {
        Ok(child) => child,
        Err(error) => {
            let message = format!("failed to spawn core process: {error}");
            let mut process = lock(state.runtime_host().process(), "core_process")?;
            process.status.state = CoreProcessState::Failed;
            process.status.last_error = Some(message.clone());
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());

            let _ = logs::append_entry(
                state,
                LogSource::Core,
                LogLevel::Error,
                message.clone(),
                Some(json!({
                    "executablePath": executable_path,
                    "args": snapshot.launch_args,
                    "workingDir": snapshot.working_dir,
                    "configPath": snapshot.config_path,
                })),
            );
            return Err(AppError::internal(message));
        }
    };

    let pid = child.id();
    if child.stdin.is_none() {
        return Err(failed_start(
            state,
            snapshot,
            child,
            None,
            AppError::internal("core process parent-lifetime pipe was not available"),
        ));
    }
    let Some(stderr) = child.stderr.take() else {
        return Err(failed_start(
            state,
            snapshot,
            child,
            None,
            AppError::internal("core process stderr pipe was not available"),
        ));
    };

    let app_handle_stderr = app_handle.clone();
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let repaired = crate::services::text_encoding::repair_utf8_mojibake(&line);
            let cleaned = strip_ansi(&repaired);
            if !cleaned.trim().is_empty() {
                let state = app_handle_stderr.state::<AppState>();
                let (level, mut fields) = parse_kernel_log_line(&cleaned);
                let message = fields
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&cleaned)
                    .to_string();
                if let Some(object) = fields.as_object_mut() {
                    object.insert("raw_line".to_string(), serde_json::Value::String(cleaned));
                }
                let _ = logs::append_entry(&state, LogSource::Core, level, message, Some(fields));
            }
        }
    });

    // Running means the IPC endpoint is healthy and belongs to this exact
    // child, including when starting after an upgrade or watchdog recovery.
    let ready = readiness::wait_for_ready(&mut child, &snapshot.endpoint).and_then(|()| {
        crate::kernel::connection::reset_endpoint(&snapshot.endpoint);
        tauri::async_runtime::block_on(crate::services::core_config::apply_active_runtime(
            state,
            crate::models::core::CoreIpcOptions {
                socket: Some(snapshot.endpoint.path.clone()),
                timeout_ms: Some(5000),
            },
        ))
    });
    if let Err(readiness_error) = ready {
        return Err(failed_start(
            state,
            snapshot,
            child,
            Some(stderr_handle),
            readiness_error,
        ));
    }

    let (status, executable_path_for_log) = {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        process.status = CoreProcessStatus {
            state: CoreProcessState::Running,
            pid: Some(pid),
            kernel: snapshot.kernel.clone(),
            executable_path: snapshot.executable_path.clone(),
            working_dir: snapshot.working_dir.clone(),
            config_path: snapshot.config_path.clone(),
            endpoint_path: snapshot.endpoint.path.clone(),
            started_at_unix_ms: Some(crate::services::common::now_unix_ms()),
            exited_at_unix_ms: None,
            exit_code: None,
            exit_reason: None,
            last_error: None,
        };
        process.endpoint = Some(snapshot.endpoint.clone());
        process.child = Some(child);
        process.stderr_handle = Some(stderr_handle);
        (
            process.status.clone(),
            process.status.executable_path.clone(),
        )
    };

    // Only advance the Client Core generation after the process has survived
    // startup and the running status has been committed.
    state.client_core_instance_started();
    let _ = app_handle.emit(
        crate::services::probe::CLIENT_CORE_UPDATED_EVENT,
        state.client_core_snapshot(),
    );

    let _ = logs::append_entry(
        state,
        LogSource::App,
        LogLevel::Info,
        "core process started".to_string(),
        Some(json!({
            "pid": pid,
            "executablePath": executable_path_for_log,
            "args": snapshot.launch_args,
        })),
    );

    Ok(status)
}

fn failed_start(
    state: &AppState,
    snapshot: &CoreConfigSnapshot,
    mut child: super::child::ManagedChild,
    mut stderr_handle: Option<std::thread::JoinHandle<()>>,
    readiness_error: AppError,
) -> AppError {
    let pid = child.id();
    let retirement = super::retire::retire_owned_child(&mut child, Some(&snapshot.endpoint.path));
    let exit_code = retirement
        .exit
        .as_ref()
        .ok()
        .and_then(|status| status.code());
    let mut message = format!(
        "core process failed startup readiness: {}",
        readiness_error.message
    );
    if let Err(error) = &retirement.tun_cleanup {
        message.push_str(&format!("; TUN cleanup unconfirmed: {}", error.message));
    }
    if let Err(error) = &retirement.exit {
        message.push_str(&format!("; old kernel exit unconfirmed: {error}"));
    } else if let Some(handle) = stderr_handle.take() {
        let _ = handle.join();
    }
    // A failed wait is not an exit receipt. Keep the original child, pipe
    // worker and endpoint installed so neither a user nor watchdog can replace
    // it. The host transition serializes this with every other spawn/stop.
    let mut process = state
        .runtime_host()
        .process()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    process.status.state = CoreProcessState::Failed;
    process.status.executable_path = snapshot.executable_path.clone();
    process.status.working_dir = snapshot.working_dir.clone();
    process.status.config_path = snapshot.config_path.clone();
    process.status.endpoint_path = snapshot.endpoint.path.clone();
    process.status.exit_code = exit_code;
    process.status.last_error = Some(message.clone());
    if retirement.exit.is_err() {
        process.status.pid = Some(pid);
        process.status.exited_at_unix_ms = None;
        process.endpoint = Some(snapshot.endpoint.clone());
        process.child = Some(child);
        process.stderr_handle = stderr_handle;
    } else {
        crate::kernel::connection::reset_endpoint(&snapshot.endpoint);
        process.status.pid = None;
        process.status.exited_at_unix_ms = Some(common::now_unix_ms());
        process.status.exit_reason = Some(CoreProcessExitReason::Exited);
    }
    drop(process);
    let _ = logs::append_entry(
        state,
        LogSource::Core,
        LogLevel::Error,
        message.clone(),
        Some(json!({
            "pid": pid, "exitCode": exit_code, "endpoint": snapshot.endpoint,
            "readinessError": readiness_error, "exitConfirmed": retirement.exit.is_ok(),
        })),
    );
    AppError {
        code: if retirement.exit.is_err() {
            "kernel_exit_unconfirmed"
        } else {
            "internal"
        },
        message,
        details: None,
    }
}
