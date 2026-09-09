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

    let mut child = match command.spawn() {
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
        let _ = child.kill();
        let _ = child.wait();
        return Err(AppError::internal(
            "core process parent-lifetime pipe was not available",
        ));
    }
    let stderr = child.stderr.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = child.wait();
        AppError::internal("core process stderr pipe was not available")
    })?;

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
    if let Err(readiness_error) = readiness::wait_for_ready(&mut child, &snapshot.endpoint) {
        let _ = child.kill();
        let exit_status = child.wait().ok();
        let _ = stderr_handle.join();
        let exit_code = exit_status.and_then(|status| status.code());
        let message = format!(
            "core process failed startup readiness: {} (code={})",
            readiness_error.message,
            exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        {
            let mut process = lock(state.runtime_host().process(), "core_process")?;
            process.status.state = CoreProcessState::Failed;
            process.status.pid = None;
            process.status.executable_path = snapshot.executable_path.clone();
            process.status.working_dir = snapshot.working_dir.clone();
            process.status.config_path = snapshot.config_path.clone();
            process.status.endpoint_path = snapshot.endpoint.path.clone();
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
            process.status.exit_code = exit_code;
            process.status.exit_reason = Some(CoreProcessExitReason::Exited);
            process.status.last_error = Some(message.clone());
        }
        let _ = logs::append_entry(
            state,
            LogSource::Core,
            LogLevel::Error,
            message.clone(),
            Some(json!({
                "pid": pid,
                "exitCode": exit_code,
                "configPath": snapshot.config_path,
                "endpoint": snapshot.endpoint,
                "readinessError": readiness_error,
            })),
        );
        return Err(AppError::internal(message));
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
