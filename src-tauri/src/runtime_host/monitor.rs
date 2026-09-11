use crate::models::{
    core_config::CoreConfigSnapshot,
    core_process::{CoreProcessExitReason, CoreProcessState},
    logs::{LogLevel, LogSource},
};
use crate::services::{common::lock, logs, system_proxy_guard};
use crate::state::app_state::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use super::spawn::spawn_core_child;
use std::time::{Duration, Instant};

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
pub(super) fn spawn_monitor(
    app_handle: AppHandle,
    snapshot: CoreConfigSnapshot,
    monitor_generation: u64,
) {
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
                if state.core_process_monitor_generation() != monitor_generation {
                    return;
                }

                let Ok(_configuration_operation) = state.proxy_config_operation().try_lock() else {
                    continue;
                };
                let Some(_host_operation) =
                    state.runtime_host().monitor_operation(monitor_generation)
                else {
                    if state.core_process_monitor_generation() != monitor_generation {
                        return;
                    }
                    continue;
                };

                // Periodic health ping: catch a kernel that is alive as a
                // process but hung on IPC (invisible to try_wait). A run of
                // failures ⇒ kill so the exit path below restarts it.
                if last_ping_at.elapsed() >= PING_INTERVAL {
                    last_ping_at = Instant::now();
                    if state.is_shutting_down() {
                        return;
                    }
                    match tauri::async_runtime::block_on(crate::kernel::connection::ensure_healthy(
                        endpoint.clone(),
                        PING_TIMEOUT,
                        PING_INTERVAL,
                    )) {
                        Ok(reconnected) => {
                            if reconnected {
                                let _ = logs::append_entry(
                                    &state,
                                    LogSource::App,
                                    LogLevel::Warn,
                                    "kernel IPC shared connection recovered".to_string(),
                                    None,
                                );
                            }
                            ping_failures = 0;
                        }
                        Err(error) => {
                            ping_failures += 1;
                            eprintln!(
                                "[ZNet] watchdog: shared kernel IPC health check failed ({}/{}): {}",
                                ping_failures, MAX_PING_FAILURES, error.message
                            );
                            if ping_failures >= MAX_PING_FAILURES {
                                let _ = logs::append_entry(
                                    &state,
                                    LogSource::App,
                                    LogLevel::Error,
                                    format!(
                                        "kernel unresponsive for {} consecutive shared IPC checks; killing to force restart",
                                        ping_failures
                                    ),
                                    None,
                                );
                                ping_failures = 0;
                                if let Ok(mut process) =
                                    lock(state.runtime_host().process(), "core_process")
                                {
                                    if state.is_shutting_down()
                                        || state.core_process_monitor_generation()
                                            != monitor_generation
                                    {
                                        return;
                                    }
                                    if let Some(child) = process.child.as_mut() {
                                        let _ = child.kill();
                                    }
                                }
                            }
                        }
                    }
                }

                let mut process = match lock(state.runtime_host().process(), "core_process") {
                    Ok(p) => p,
                    Err(_) => return,
                };
                // The generation may have changed after the pre-lock check
                // while this monitor was waiting for the process mutex.
                // Re-check under the same lock used by start() before
                // interpreting an empty child slot as an unexpected exit.
                if state.core_process_monitor_generation() != monitor_generation {
                    return;
                }
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
                process.status.pid = None;
                if let Some(endpoint) = process.endpoint.take() {
                    crate::kernel::connection::reset_endpoint(&endpoint);
                }
                let stderr_handle = process.stderr_handle.take();
                drop(process);
                if let Some(handle) = stderr_handle {
                    let _ = handle.join();
                }

                state.client_core_instance_lost();
                let _ = app_handle.emit(
                    crate::services::probe::CLIENT_CORE_UPDATED_EVENT,
                    state.client_core_snapshot(),
                );

                if let Err(error) = system_proxy_guard::disable_with_guard() {
                    state
                        .runtime_host()
                        .record_error(format!("proxy cleanup after core exit: {}", error.message));
                }
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
            if state.core_process_monitor_generation() != monitor_generation {
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
                    state.runtime_host().record_error(msg.clone());
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
                if state.core_process_monitor_generation() != monitor_generation {
                    return;
                }
                let _configuration_operation = state.proxy_config_operation().blocking_lock();
                if state.is_shutting_down()
                    || state.core_process_monitor_generation() != monitor_generation
                {
                    return;
                }
                let Some(mut operation) =
                    state.runtime_host().monitor_operation(monitor_generation)
                else {
                    continue;
                };
                let result = spawn_core_child(state.inner(), &snapshot, &app_handle);
                operation.finish(&result);
                match result {
                    Ok(_) => {
                        last_started_at = Instant::now();
                        let _ = app_handle.emit(
                            "core:process-restarted",
                            json!({ "attempt": restart_budget }),
                        );
                        crate::services::network_probe::emit_host_network_changed(
                            &app_handle,
                            "core.watchdog_restarted",
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
