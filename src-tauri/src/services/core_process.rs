use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::services::common;

use serde_json::json;
use tauri::Manager;
use tauri::{AppHandle, Emitter, State};

use crate::errors::{AppError, AppResult};
use crate::models::{
    core::CoreEndpoint,
    core_config::CoreConfigSnapshot,
    core_process::{CoreProcessExitReason, CoreProcessState, CoreProcessStatus},
    logs::{LogLevel, LogSource},
};
use crate::services::{common::lock, core_config, local_proxy, logs, system_proxy_guard};
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

/// Kill the kernel by its default binary name (`zero.exe` / `zero`).
///
/// Unlike [`kill_external`] this needs no [`AppState`], so the shutdown
/// coordinator can call it from a bare `Fn()` callback. This guarantees the
/// kernel exits with the GUI even when the managed child handle is gone —
/// e.g. the GUI connected to an already-running (external) kernel and never
/// owned a child, or `ManagedCoreProcess::Drop` didn't run because the
/// process exited without full Rust destructor unwinding.
pub fn kill_core_default() {
    let name = if cfg!(windows) { "zero.exe" } else { "zero" };
    #[cfg(windows)]
    {
        let _ = common::background_command("taskkill")
            .args(["/F", "/IM", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = common::background_command("pkill")
            .args(["-9", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    // Give the OS a moment to release the pipe handle.
    std::thread::sleep(std::time::Duration::from_millis(300));
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
            app_config.core.config_path = Some(temp_path.to_string_lossy().to_string());
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

    // Pre-check the local proxy port before spawning. If something else
    // already occupies it (another proxy, a stale process), the kernel
    // will fail to bind and die immediately — which previously cascaded
    // into a destructive system-proxy disable. Surface the conflict now.
    let (proxy_host, proxy_port) = {
        let config = lock(state.app_config(), "app_config")?;
        (config.local_proxy.host.clone(), config.local_proxy.port)
    };
    if let Err(error) = local_proxy::check_port_available(&proxy_host, proxy_port) {
        let message = error.message.clone();
        let _ = logs::append_entry(
            state.inner(),
            LogSource::App,
            LogLevel::Error,
            format!("core process: {message}"),
            Some(json!({ "host": proxy_host, "port": proxy_port })),
        );
        let mut process = lock(state.core_process(), "core_process")?;
        process.status.state = CoreProcessState::Failed;
        process.status.last_error = Some(message);
        process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
        return Err(error);
    }

    let status = spawn_core_child(state.inner(), &snapshot, &app_handle)?;
    // Spawn the watchdog once. It reuses spawn_core_child to restart after
    // crashes, so on restart it must NOT spawn another monitor — that would
    // multiply monitors on every crash.
    spawn_monitor(app_handle.clone(), snapshot);
    Ok(status)
}

/// Spawn the kernel child process, wire up its stderr pump, and record the
/// running status. Shared between the initial [`start`] and the watchdog's
/// crash-restart path so neither duplicates the other's bookkeeping.
///
/// Does NOT spawn the watchdog monitor — [`start`] does that exactly once,
/// and the monitor reuses this function to restart without spawning another
/// monitor (which would otherwise multiply on every crash).
fn spawn_core_child(
    state: &AppState,
    snapshot: &CoreConfigSnapshot,
    app_handle: &AppHandle,
) -> AppResult<CoreProcessStatus> {
    let executable_path = snapshot.executable_path.as_deref().unwrap_or_default();

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

    let mut command = common::background_command(executable_path);
    command.args(&snapshot.launch_args);
    if let Some(working_dir) = snapshot.working_dir.as_deref() {
        command.current_dir(working_dir);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let message = format!("failed to spawn core process: {error}");
            let mut process = lock(state.core_process(), "core_process")?;
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
                    let _ =
                        logs::append_entry(&state, LogSource::Core, level, cleaned, Some(fields));
                }
            }
        }
    });

    let (status, executable_path_for_log) = {
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
        (
            process.status.clone(),
            process.status.executable_path.clone(),
        )
    };

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

/// Background watchdog for the kernel child process.
///
/// Polls the child for exit and, when it dies unexpectedly (crash / abnormal
/// exit), restarts it with backoff up to a capped number of attempts. The
/// loop bails out — without restarting — when:
///   - the user explicitly stopped the kernel (`exit_reason == Stopped`),
///   - app shutdown has begun (`AppState::is_shutting_down`),
///   - the crash-restart budget is exhausted.
///
/// A run lasting at least `STABLE_RUN` resets the restart budget, so a
/// kernel that flaps occasionally still recovers, while a genuinely broken
/// kernel eventually gives up instead of looping forever.
fn spawn_monitor(app_handle: AppHandle, snapshot: CoreConfigSnapshot) {
    /// How often to poll the child for exit.
    const POLL_INTERVAL: Duration = Duration::from_secs(1);
    /// Cooldown between a crash and the next restart attempt.
    const RESTART_BACKOFF: Duration = Duration::from_secs(3);
    /// Max consecutive crash-restarts before giving up.
    const MAX_RESTARTS: u32 = 5;
    /// A run lasting at least this long counts as "stable" — the next crash
    /// resets the restart budget instead of accumulating toward the cap.
    const STABLE_RUN: Duration = Duration::from_secs(60);
    /// How often to ping the kernel to detect a hung-but-alive process.
    const PING_INTERVAL: Duration = Duration::from_secs(10);
    /// Per-ping timeout. Short, so a hung kernel is detected without long
    /// stalls in the poll loop.
    const PING_TIMEOUT: Duration = Duration::from_secs(2);
    /// Consecutive ping failures before the kernel is considered hung and
    /// killed (so the exit path restarts it).
    const MAX_PING_FAILURES: u32 = 3;

    std::thread::spawn(move || {
        let state = app_handle.state::<AppState>();
        let endpoint = snapshot.endpoint.clone();
        let mut restart_budget: u32 = 0;
        let mut last_started_at = Instant::now();
        let mut last_ping_at = Instant::now();
        let mut ping_failures: u32 = 0;

        'outer: loop {
            // ── Phase 1: monitor the current child until it exits ──
            let was_stable = loop {
                std::thread::sleep(POLL_INTERVAL);
                if state.is_shutting_down() {
                    return;
                }

                // Periodic health ping: catch a kernel that is alive as a
                // process but hung on IPC (invisible to try_wait). A run of
                // failures ⇒ kill so the exit path below restarts it.
                if last_ping_at.elapsed() >= PING_INTERVAL {
                    last_ping_at = Instant::now();
                    if state.is_shutting_down() {
                        return;
                    }
                    if ping_kernel(&endpoint, PING_TIMEOUT) {
                        ping_failures = 0;
                    } else {
                        ping_failures += 1;
                        eprintln!(
                            "[ZNet] watchdog: kernel ping failed ({}/{})",
                            ping_failures, MAX_PING_FAILURES
                        );
                        if ping_failures >= MAX_PING_FAILURES {
                            let _ = logs::append_entry(
                                &state,
                                LogSource::App,
                                LogLevel::Error,
                                format!(
                                    "kernel unresponsive for {} consecutive pings; killing to force restart",
                                    ping_failures
                                ),
                                None,
                            );
                            ping_failures = 0;
                            if let Ok(mut process) = lock(state.core_process(), "core_process") {
                                if let Some(child) = process.child.as_mut() {
                                    let _ = child.kill();
                                }
                            }
                        }
                    }
                }

                let mut process = match lock(state.core_process(), "core_process") {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let exited_info = match process.child.as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => None,
                };
                // Still running → keep polling.
                if exited_info.is_none() && process.child.is_some() {
                    continue;
                }
                // Either the child exited, or stop() took it. A user-
                // initiated stop must NOT trigger a restart.
                if process.child.is_none()
                    && matches!(
                        process.status.exit_reason,
                        Some(CoreProcessExitReason::Stopped)
                    )
                {
                    return;
                }
                let code = exited_info.as_ref().and_then(|es| es.code());
                let was_stable = last_started_at.elapsed() >= STABLE_RUN;
                let reason = if code.is_none() {
                    CoreProcessExitReason::Crashed
                } else {
                    CoreProcessExitReason::Exited
                };
                let reason_str = if code.is_none() { "crashed" } else { "exited" };
                process.status.exit_code = code;
                process.status.state = CoreProcessState::Exited;
                process.status.exit_reason = Some(reason);
                process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
                process.child = None;
                drop(process);

                let _ = system_proxy_guard::disable_with_guard();
                let msg = format!("core process {} (code={})", reason_str, code.unwrap_or(-1));
                let _ =
                    logs::append_entry(&state, LogSource::App, LogLevel::Warn, msg.clone(), None);
                let _ = app_handle.emit(
                    "core:process-exited",
                    json!({
                        "reason": reason_str,
                        "code": code,
                        "message": msg,
                    }),
                );
                break was_stable;
            };

            // ── Phase 2: restart with backoff (handles spawn retries too) ──
            if state.is_shutting_down() {
                return;
            }
            if was_stable {
                restart_budget = 0;
                eprintln!("[ZNet] watchdog: previous run was stable, reset restart budget");
            }
            loop {
                restart_budget += 1;
                if restart_budget > MAX_RESTARTS {
                    let msg = format!(
                        "core process crashed {} times consecutively; giving up auto-restart",
                        MAX_RESTARTS
                    );
                    let _ = logs::append_entry(
                        &state,
                        LogSource::App,
                        LogLevel::Error,
                        msg.clone(),
                        None,
                    );
                    let _ = app_handle.emit(
                        "core:process-exited",
                        json!({ "reason": "gave_up", "code": null, "message": msg }),
                    );
                    return;
                }
                eprintln!(
                    "[ZNet] watchdog: kernel exited, restarting in {:?} (attempt {}/{})",
                    RESTART_BACKOFF, restart_budget, MAX_RESTARTS
                );
                let _ = logs::append_entry(
                    &state,
                    LogSource::App,
                    LogLevel::Info,
                    format!(
                        "core process: auto-restart in {:?} (attempt {}/{})",
                        RESTART_BACKOFF, restart_budget, MAX_RESTARTS
                    ),
                    None,
                );
                std::thread::sleep(RESTART_BACKOFF);
                if state.is_shutting_down() {
                    return;
                }
                match spawn_core_child(state.inner(), &snapshot, &app_handle) {
                    Ok(_) => {
                        last_started_at = Instant::now();
                        let _ = app_handle.emit(
                            "core:process-restarted",
                            json!({ "attempt": restart_budget }),
                        );
                        continue 'outer; // back to Phase 1 with the new child
                    }
                    Err(error) => {
                        eprintln!(
                            "[ZNet] watchdog: restart spawn failed: {:?}; will retry",
                            error
                        );
                        // spawn_core_child already set status=Failed. Loop:
                        // budget++ and another backed-off retry.
                    }
                }
            }
        }
    });
}

/// Send a short-lived `ping` to the kernel to check it is responsive on IPC.
///
/// Uses a fresh connection (not the multiplexed one) so a hung kernel can't
/// poison the shared connection — the kernel closes non-subscribe
/// connections right after responding.
fn ping_kernel(endpoint: &CoreEndpoint, timeout: Duration) -> bool {
    let frame = serde_json::json!({"type":"ping"});
    let Ok(frame_bytes) = crate::kernel::transport::serialize_frame(&frame) else {
        return false;
    };
    crate::kernel::transport::send_json_line_request(endpoint.clone(), frame_bytes, timeout).is_ok()
}

pub fn stop(state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    let proxy_result = system_proxy_guard::disable_with_guard();
    // Drop the multiplexed connection so the next request opens a fresh one
    // instead of reusing a handle whose peer (the kernel) is about to die.
    crate::kernel::connection::reset();
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
        if (i == 0 || bytes[i - 1] == b' ')
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
        {
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

            msg_without_kv.push_str(&msg[key_start..i]);
            continue;
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

#[cfg(test)]
mod tests {
    use super::parse_kernel_log_line;
    use crate::models::logs::LogLevel;

    #[test]
    fn parse_kernel_log_line_keeps_plain_trailing_word() {
        let (level, fields) =
            parse_kernel_log_line("2026-06-10T10:33:23.610135Z INFO kernel started");

        assert_eq!(level, LogLevel::Info);
        assert_eq!(fields["message"], "kernel started");
        assert_eq!(fields["timestamp"], "2026-06-10T10:33:23.610135Z");
        assert_eq!(fields["level"], "INFO");
    }

    #[test]
    fn parse_kernel_log_line_extracts_fields_without_dropping_message_words() {
        let (_, fields) = parse_kernel_log_line(
            "2026-06-10T10:33:23.610135Z INFO ipc client connected pipe=\\\\.\\pipe\\zero-control active=3",
        );

        assert_eq!(fields["message"], "ipc client connected");
        assert_eq!(fields["pipe"], "\\\\.\\pipe\\zero-control");
        assert_eq!(fields["active"], "3");
    }
}
