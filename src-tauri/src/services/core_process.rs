use std::io::{BufRead, BufReader};
use std::process::Stdio;

use crate::services::common;

use serde_json::json;
use tauri::Manager;
use tauri::{AppHandle, Emitter, State};

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
    let config = {
        lock(state.app_config(), "app_config")?.core.clone()
    };
    let snapshot = core_config::snapshot_from_config(&config)?;

    // Require config_path — the kernel's `run` command needs a config file
    if snapshot.config_path.is_none() {
        let has_active = lock(state.proxy_configs(), "proxy_config")?
            .iter()
            .any(|p| p.active);
        let hint = if has_active {
            "有活跃代理配置但尚未导出，请先设置代理模式"
        } else {
            "没有活跃代理配置，请先创建并激活代理配置"
        };
        return Err(AppError::invalid_argument(hint));
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

    let mut command = common::background_command(executable_path);
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

            let app_handle_stderr = app_handle.clone();
            let stderr_handle = std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        let cleaned = strip_ansi(&line);
                        if !cleaned.trim().is_empty() {
                            let state = app_handle_stderr.state::<AppState>();
                            let _ = logs::append_entry(
                                &state,
                                LogSource::Core,
                                LogLevel::Error,
                                cleaned,
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
            process.child = Some(child);
            process.stderr_handle = Some(stderr_handle);

            // Monitor thread: poll child process exit so UI updates in real time
            let app_handle_mon = app_handle.clone();
            std::thread::spawn(move || {
                let state = app_handle_mon.state::<AppState>();
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let mut process = match lock(state.core_process(), "core_process") {
                        Ok(p) => p,
                        Err(_) => break,
                    };
                    let exited = if let Some(ref mut child) = process.child {
                        matches!(child.try_wait(), Ok(Some(_)))
                    } else {
                        true // child already removed
                    };
                    if exited {
                        let code = process.status.exit_code;
                        let reason = if code == Some(0) { CoreProcessExitReason::Exited } else { CoreProcessExitReason::Crashed };
                        let reason_str = if code == Some(0) { "exited" } else { "crashed" };
                        process.status.state = CoreProcessState::Exited;
                        process.status.exit_reason = Some(reason);
                        process.status.exited_at_unix_ms =
                            Some(crate::services::common::now_unix_ms());
                        process.child = None;
                        drop(process);
                        let msg = format!("core process {} (code={})", reason_str, code.unwrap_or(-1));
                        let _ = logs::append_entry(&state, LogSource::App, LogLevel::Warn, msg.clone(), None);
                        // Notify frontend so UI updates in real time
                        let _ = app_handle_mon.emit("core:process-exited", json!({
                            "reason": reason_str,
                            "code": code,
                            "message": msg,
                        }));
                        break;
                    }
                }
            });

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

/// Remove ANSI escape codes (CSI sequences) from stderr output.
/// Matches patterns like ESC[...m, ESC[...K, etc.
fn strip_ansi(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Skip ESC[ ... until a terminal byte (letter A-Z or a-z)
            i += 2;
            while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // skip the terminal byte
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}
