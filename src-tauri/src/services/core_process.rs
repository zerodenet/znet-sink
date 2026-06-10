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
use crate::services::{common::lock, core_config, logs, system_proxy_guard};
use crate::state::app_state::AppState;

pub fn status(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    refresh_status(state.inner())
}

/// Kill any core process left by a previous session that we don't own.
///
/// Uses OS-level force-kill (`taskkill` on Windows, `pkill` on Unix) to
/// terminate the core executable, then waits briefly for the OS and the
/// named pipe server to release handles.
pub(crate) fn kill_external(state: &AppState) -> AppResult<()> {
    let config = { lock(state.app_config(), "app_config")?.core.clone() };
    let snapshot = core_config::snapshot_from_config(&config)?;

    let configured_name = snapshot
        .executable_path
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string());

    // Always try to kill the default binary name as a fallback — the
    // kernel may have been started by auto_start with a path that
    // isn't (yet) recorded in the config.
    let fallback = if cfg!(windows) {
        "zero.exe".to_string()
    } else {
        "zero".to_string()
    };

    let executable_name = configured_name.as_deref().unwrap_or(&fallback);

    #[cfg(windows)]
    {
        let _ = common::background_command("taskkill")
            .args(["/F", "/IM", executable_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(not(windows))]
    {
        let _ = common::background_command("pkill")
            .args(["-9", executable_name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    // Give the OS and pipe server time to release handles before we
    // try to spawn a new process.
    std::thread::sleep(std::time::Duration::from_millis(500));

    Ok(())
}

pub fn start(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    let has_active_proxy_config = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .any(|profile| profile.active);
    if has_active_proxy_config {
        core_config::export_active(state.clone())?;
    }

    let config = { lock(state.app_config(), "app_config")?.core.clone() };
    let snapshot = core_config::snapshot_from_config(&config)?;

    // When no config_path exists, generate a minimal temp config so the
    // kernel can start with its control plane enabled. The kernel enters
    // a "waiting for config" state, and the GUI shows appropriate status.
    let snapshot = if snapshot.config_path.is_none() {
        let temp_path = core_config::write_minimal_temp_config()?;
        {
            let mut app_config = lock(state.app_config(), "app_config")?;
            app_config.core.config_path = Some(
                temp_path.to_string_lossy().to_string(),
            );
            let config_path = crate::services::app_config_store::default_config_path()?;
            crate::services::app_config_store::save(&config_path, &app_config)?;
        }
        let config = { lock(state.app_config(), "app_config")?.core.clone() };
        core_config::snapshot_from_config(&config)?
    } else {
        snapshot
    };

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

    // Kill any stale core process left by a previous GUI session (crashed
    // tab, force-quit, etc.).  Without this the new core can't bind the
    // named pipe and the whole flow blocks.
    let _ = kill_external(state.inner());

    let _ = logs::append_entry(
        state.inner(),
        LogSource::App,
        LogLevel::Info,
        format!(
            "core process: spawning {} with args {:?}",
            executable_path, snapshot.launch_args
        ),
        None,
    );

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
                            let (level, fields) = parse_kernel_log_line(&cleaned);
                            let _ = logs::append_entry(
                                &state,
                                LogSource::Core,
                                level,
                                cleaned,
                                Some(fields),
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
                    let exited_info = if let Some(ref mut child) = process.child {
                        match child.try_wait() {
                            Ok(Some(es)) => Some(es),
                            _ => None,
                        }
                    } else {
                        // child already removed — synthesize an exit status so we still clean up
                        None
                    };
                    if exited_info.is_some() || process.child.is_none() {
                        // If stop() already took the child and set the exit
                        // reason, don't overwrite it with a false "crashed"
                        // event.  The status was handled in stop() above.
                        if process.child.is_none()
                            && matches!(
                                process.status.exit_reason,
                                Some(CoreProcessExitReason::Stopped)
                            )
                        {
                            break;
                        }

                        let code = exited_info.as_ref().and_then(|es| es.code());
                        // code == None ⇒ killed by signal (Unix) → crashed
                        // code == Some(_) ⇒ normal exit (any code, including 1) → exited
                        let reason = if code.is_none() {
                            CoreProcessExitReason::Crashed
                        } else {
                            CoreProcessExitReason::Exited
                        };
                        let reason_str = if code.is_none() { "crashed" } else { "exited" };
                        process.status.exit_code = code;
                        process.status.state = CoreProcessState::Exited;
                        process.status.exit_reason = Some(reason);
                        process.status.exited_at_unix_ms =
                            Some(crate::services::common::now_unix_ms());
                        process.child = None;
                        drop(process);
                        let _ = system_proxy_guard::disable_with_guard();
                        let msg =
                            format!("core process {} (code={})", reason_str, code.unwrap_or(-1));
                        let _ = logs::append_entry(
                            &state,
                            LogSource::App,
                            LogLevel::Warn,
                            msg.clone(),
                            None,
                        );
                        // Notify frontend so UI updates in real time
                        let _ = app_handle_mon.emit(
                            "core:process-exited",
                            json!({
                                "reason": reason_str,
                                "code": code,
                                "message": msg,
                            }),
                        );
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
            let message = format!("failed to spawn core process: {error}");
            let mut process = lock(state.core_process(), "core_process")?;
            process.status.state = CoreProcessState::Failed;
            process.status.last_error = Some(message.clone());
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());

            logs::append_entry(
                state.inner(),
                LogSource::Core,
                LogLevel::Error,
                message.clone(),
                Some(json!({
                    "executablePath": executable_path,
                    "args": snapshot.launch_args,
                    "workingDir": snapshot.working_dir,
                    "configPath": snapshot.config_path,
                })),
            )?;

            Err(AppError::internal(message))
        }
    }
}

pub fn stop(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    let proxy_result = system_proxy_guard::disable_with_guard();
    let (child, stderr_handle) = {
        let mut process = lock(state.core_process(), "core_process")?;
        refresh_locked_status(&mut process, state.inner())?;
        (process.child.take(), process.stderr_handle.take())
    };

    let Some(mut child) = child else {
        // No managed child — but the kernel might be an external process
        // (e.g. started by a previous GUI session).  Force-kill it so the
        // UI "stop" button actually works in that scenario.
        let _ = kill_external(state.inner());
        proxy_result?;
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

            proxy_result?;
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
                // code == None ⇒ killed by signal (Unix) → crashed
                // code == Some(_) ⇒ normal exit (any code) → exited
                process.status.exit_reason = if status.code().is_none() {
                    Some(CoreProcessExitReason::Crashed)
                } else {
                    Some(CoreProcessExitReason::Exited)
                };
            }
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
            process.child = None;

            if let Some(handle) = process.stderr_handle.take() {
                let _ = handle.join();
            }

            let _ = system_proxy_guard::disable_with_guard();

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

/// Classify a core stderr line into a log level based on content heuristics.
fn classify_stderr_level(line: &str) -> LogLevel {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("panic") {
        LogLevel::Error
    } else if lower.contains("warn") {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

/// Parse a kernel log line into a structured level and fields.
///
/// The kernel uses a `tracing-subscriber` style format:
///   `2026-06-10T10:33:23.610135Z  INFO ipc client connected pipe=... active=3`
///
/// Extracts the timestamp and level, then converts `key=value` pairs
/// into JSON fields so the frontend can display them in a structured way.
fn parse_kernel_log_line(line: &str) -> (LogLevel, serde_json::Value) {
    let mut fields = serde_json::Map::new();

    // Try to parse the tracing-subscriber format:
    //   <ISO 8601 timestamp>  <LEVEL> <spans...> <message key=value ...>
    let trimmed = line.trim();

    // Split off the ISO 8601 timestamp: YYYY-MM-DDTHH:MM:SS(.fff)?Z
    let (ts_rest, level, msg) = if trimmed.len() >= 20
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(10) == Some(&b'T')
    {
        // Find end of timestamp (next space)
        let ts_end = trimmed.find(' ').unwrap_or(trimmed.len());
        let ts = &trimmed[..ts_end];
        fields.insert(
            "timestamp".to_string(),
            serde_json::Value::String(ts.to_string()),
        );

        let after_ts = trimmed[ts_end..].trim_start();

        // Next token is the level: INFO, WARN, ERROR, DEBUG, TRACE
        let (lv, rest) = if after_ts.len() >= 4 {
            let level_end = after_ts.find(' ').unwrap_or(after_ts.len());
            (&after_ts[..level_end], after_ts[level_end..].trim_start())
        } else {
            ("INFO", after_ts)
        };
        fields.insert(
            "level".to_string(),
            serde_json::Value::String(lv.to_string()),
        );

        (true, lv, rest)
    } else {
        (false, "", trimmed)
    };

    let level = if ts_rest {
        match level {
            "ERROR" => LogLevel::Error,
            "WARN" => LogLevel::Warn,
            "DEBUG" | "TRACE" => LogLevel::Debug,
            _ => LogLevel::Info,
        }
    } else {
        classify_stderr_level(line)
    };

    // Extract key=value pairs from the message
    // The kernel uses `key=value` format for structured fields, e.g.:
    //   `pipe=\\.\pipe\zero-control active=3`
    // keys are alphanumeric with underscores; values are everything until
    // the next space or end of line.
    let mut msg_without_kv = String::new();
    let mut i = 0;
    let bytes = msg.as_bytes();
    while i < bytes.len() {
        // Check for key=value pattern at current position
        if i == 0 || bytes[i - 1] == b' ' {
            let key_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'=' && i > key_start {
                let key = &msg[key_start..i];
                i += 1; // skip '='

                // Value: until next space or end of line
                // Handle `\\.\pipe\...` paths (backslash escapes)
                let val_start = i;
                while i < bytes.len() {
                    if bytes[i] == b' ' {
                        // Check if this is likely part of a path
                        // (backslash before space, or we're in key=value chain)
                        if i + 1 < bytes.len()
                            && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'_')
                        {
                            // Next token looks like another key — end of value
                            break;
                        }
                        // Otherwise it's a regular space, also end
                        break;
                    }
                    i += 1;
                }
                let value = &msg[val_start..i];
                fields.insert(
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                );

                // Skip trailing space
                if i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                continue;
            }
        }

        // Regular character — append to message
        msg_without_kv.push(bytes[i] as char);
        i += 1;
    }

    let clean_msg = msg_without_kv.trim().to_string();
    fields.insert(
        "message".to_string(),
        serde_json::Value::String(if clean_msg.is_empty() {
            msg.to_string()
        } else {
            clean_msg
        }),
    );

    (level, serde_json::Value::Object(fields))
}
