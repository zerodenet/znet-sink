use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use serde_json::json;
use tauri::Manager;
use tauri::{AppHandle, State};

use crate::errors::{AppError, AppResult};
use crate::models::{
    core_process::{CoreProcessExitReason, CoreProcessState, CoreProcessStatus},
    logs::{LogLevel, LogSource},
};
use crate::services::{common::lock, core_config, logs};
use crate::state::app_state::AppState;

pub fn status(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    refresh_status(state.inner())
}

pub fn start(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    let snapshot = core_config::snapshot_from_config(&config)?;

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
    let executable_path = snapshot.executable_path.as_deref().unwrap_or_default();

    {
        let mut process = lock(state.core_process(), "core_process")?;
        refresh_locked_status(&mut process, state.inner())?;
        if process.child.is_some() {
            return Ok(process.status.clone());
        }

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

    let mut command = Command::new(executable_path);
    command.args(&snapshot.launch_args);
    if let Some(working_dir) = snapshot.working_dir.as_deref() {
        command.current_dir(working_dir);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());

    match command.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            let stderr = child.stderr.take().unwrap();
            let app_handle_clone = app_handle.clone();

            let stderr_handle = std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        if !line.trim().is_empty() {
                            let state = app_handle_clone.state::<AppState>();
                            let _ = logs::append_entry(
                                &state,
                                LogSource::Core,
                                LogLevel::Error,
                                line,
                                None,
                            );
                        }
                    }
                }
            });

            let mut process = lock(state.core_process(), "core_process")?;
            process.status = CoreProcessStatus {
                state: CoreProcessState::Running,
                pid: Some(pid),
                kernel: snapshot.kernel,
                executable_path: snapshot.executable_path,
                working_dir: snapshot.working_dir,
                config_path: snapshot.config_path,
                endpoint_path: snapshot.endpoint.path,
                started_at_unix_ms: Some(crate::services::common::now_unix_ms()),
                exited_at_unix_ms: None,
                exit_code: None,
                exit_reason: None,
                last_error: None,
            };
            process.child = Some(child);
            process.stderr_handle = Some(stderr_handle);

            logs::append_entry(
                state.inner(),
                LogSource::App,
                LogLevel::Info,
                "core process started".to_string(),
                Some(json!({
                    "pid": pid,
                    "executablePath": process.status.executable_path,
                    "args": snapshot.launch_args,
                })),
            )?;

            Ok(process.status.clone())
        }
        Err(error) => {
            let message = format!("failed to start core process: {error}");
            let mut process = lock(state.core_process(), "core_process")?;
            process.status.state = CoreProcessState::Failed;
            process.status.last_error = Some(message.clone());
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());

            logs::append_entry(
                state.inner(),
                LogSource::Core,
                LogLevel::Error,
                message.clone(),
                Some(json!({ "executablePath": executable_path })),
            )?;

            Err(AppError::internal(message))
        }
    }
}

pub fn stop(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    let (child, stderr_handle) = {
        let mut process = lock(state.core_process(), "core_process")?;
        refresh_locked_status(&mut process, state.inner())?;
        (process.child.take(), process.stderr_handle.take())
    };

    let Some(mut child) = child else {
        return refresh_status(state.inner());
    };

    let pid = child.id();
    let kill_result = child.kill();
    let wait_result = child.wait();

    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    let mut process = lock(state.core_process(), "core_process")?;
    match (kill_result, wait_result) {
        (Ok(()), Ok(status)) => {
            process.status.state = CoreProcessState::Exited;
            process.status.pid = None;
            process.status.exit_code = status.code();
            process.status.exit_reason = Some(CoreProcessExitReason::Stopped);
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
            process.status.last_error = None;

            logs::append_entry(
                state.inner(),
                LogSource::App,
                LogLevel::Info,
                "core process stopped".to_string(),
                Some(json!({ "pid": pid, "exitCode": status.code() })),
            )?;

            Ok(process.status.clone())
        }
        (Err(error), _) | (_, Err(error)) => {
            let message = format!("failed to stop core process: {error}");
            process.status.state = CoreProcessState::Failed;
            process.status.last_error = Some(message.clone());

            logs::append_entry(
                state.inner(),
                LogSource::App,
                LogLevel::Error,
                message.clone(),
                Some(json!({ "pid": pid })),
            )?;

            Err(AppError::internal(message))
        }
    }
}

pub(crate) fn refresh_status(state: &AppState) -> AppResult<CoreProcessStatus> {
    let mut process = lock(state.core_process(), "core_process")?;
    refresh_locked_status(&mut process, state)?;
    Ok(process.status.clone())
}

fn refresh_locked_status(
    process: &mut crate::state::app_state::ManagedCoreProcess,
    state: &AppState,
) -> AppResult<()> {
    let Some(child) = process.child.as_mut() else {
        return Ok(());
    };

    let pid = child.id();
    match child.try_wait() {
        Ok(Some(status)) => {
            process.status.state = CoreProcessState::Exited;
            process.status.pid = None;
            process.status.exit_code = status.code();
            // Only set exit_reason if not already set by stop()
            if process.status.exit_reason.is_none() {
                process.status.exit_reason = if status.success() {
                    Some(CoreProcessExitReason::Exited)
                } else {
                    Some(CoreProcessExitReason::Crashed)
                };
            }
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
            process.child = None;

            if let Some(handle) = process.stderr_handle.take() {
                let _ = handle.join();
            }

            let (level, message) =
                if process.status.exit_reason == Some(CoreProcessExitReason::Crashed) {
                    (
                        LogLevel::Error,
                        format!(
                            "core process crashed, pid: {pid}, exit code: {}",
                            status.code().unwrap_or(-1)
                        ),
                    )
                } else {
                    (
                        LogLevel::Info,
                        format!(
                            "core process exited, pid: {pid}, exit code: {}",
                            status.code().unwrap_or(-1)
                        ),
                    )
                };

            let _ = logs::append_entry(
                state,
                LogSource::App,
                level,
                message,
                Some(
                    serde_json::json!({ "pid": pid, "exitCode": status.code(), "exitReason": process.status.exit_reason }),
                ),
            );
        }
        Ok(None) => {
            process.status.state = CoreProcessState::Running;
            process.status.pid = Some(pid);
        }
        Err(error) => {
            process.status.state = CoreProcessState::Failed;
            process.status.last_error = Some(format!("failed to poll core process: {error}"));

            let _ = logs::append_entry(
                state,
                LogSource::App,
                LogLevel::Error,
                format!("failed to poll core process: {error}"),
                Some(serde_json::json!({ "pid": pid })),
            );
        }
    }

    Ok(())
}
