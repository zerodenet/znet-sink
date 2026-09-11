use crate::errors::{AppError, AppResult};
use crate::models::{
    core_process::{CoreProcessExitReason, CoreProcessState, CoreProcessStatus},
    logs::{LogLevel, LogSource},
};
use crate::services::{common::lock, logs, system_proxy_guard};
use crate::state::app_state::AppState;
use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use super::status::{refresh_locked_status, refresh_status};
use std::time::{Duration, Instant};

pub fn stop(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    state.runtime_host().run("stop", false, || {
        stop_with_proxy_restore(app_handle.clone(), state.clone(), true)
    })
}

/// Stop the managed kernel without changing a GUI-owned system proxy.
///
/// This is only for short internal transitions (restart or in-place kernel
/// replacement) where the same local proxy endpoint will be available again.
pub(crate) fn stop_preserving_system_proxy(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<CoreProcessStatus> {
    state.runtime_host().run("stop_for_restart", false, || {
        stop_with_proxy_restore(app_handle.clone(), state.clone(), false)
    })
}

pub(super) fn stop_with_proxy_restore(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    restore_system_proxy: bool,
) -> AppResult<CoreProcessStatus> {
    // Cancel the old watchdog even when the child already exited or is in backoff.
    state.next_core_process_monitor_generation();
    let proxy_result = if restore_system_proxy {
        system_proxy_guard::disable_with_guard()
    } else {
        Ok(())
    };
    let (child, mut stderr_handle) = {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        refresh_locked_status(&mut process, state.inner())?;
        if process.child.is_some() {
            // Mark the stop intent before releasing the child handle.
            // The watchdog polls `child.is_none()` once per second; without
            // this early marker it can see "no child + no Stopped reason"
            // in the small window before we finish kill/wait bookkeeping and
            // misclassify a user-initiated stop/restart as a crash.
            process.status.exit_reason = Some(CoreProcessExitReason::Stopped);
            process.status.last_error = None;
        }
        // Retire only the endpoint recorded when this owned child became ready.
        if let Some(endpoint) = process.endpoint.as_ref() {
            crate::kernel::connection::reset_endpoint(endpoint);
        }
        process.status.exit_reason = Some(CoreProcessExitReason::Stopped);
        (process.child.take(), process.stderr_handle.take())
    };

    let Some(mut child) = child else {
        // The GUI only controls children it owns. An independently launched
        // Zero process is deliberately outside this lifecycle.
        proxy_result?;
        let status = refresh_status(state.inner());
        return status;
    };

    let pid = child.id();
    let wait_result = stop_owned_child(&mut child, Duration::from_secs(5));

    if wait_result.is_ok() {
        if let Some(handle) = stderr_handle.take() {
            let _ = handle.join();
        }
    }

    let mut process = lock(state.runtime_host().process(), "core_process")?;
    match wait_result {
        Ok(status) => {
            process.endpoint = None;
            process.status.state = CoreProcessState::Exited;
            process.status.pid = None;
            process.status.exit_code = status.code();
            process.status.exit_reason = Some(CoreProcessExitReason::Stopped);
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
            process.status.last_error = None;

            let _ = logs::append_entry(
                state.inner(),
                LogSource::App,
                LogLevel::Info,
                "core process stopped".to_string(),
                Some(json!({ "pid": pid, "exitCode": status.code() })),
            );

            let status = process.status.clone();
            drop(process);
            state.client_core_instance_lost();
            let _ = app_handle.emit(
                crate::services::probe::CLIENT_CORE_UPDATED_EVENT,
                state.client_core_snapshot(),
            );
            proxy_result?;
            Ok(status)
        }
        Err(error) => {
            let message = format!("failed to stop core process: {error}");
            process.child = Some(child);
            process.stderr_handle = stderr_handle;
            process.status.state = CoreProcessState::Failed;
            process.status.last_error = Some(message.clone());

            let _ = logs::append_entry(
                state.inner(),
                LogSource::App,
                LogLevel::Error,
                message.clone(),
                Some(json!({ "pid": pid })),
            );

            Err(AppError::internal(message))
        }
    }
}

/// Ask Zero to stop by closing the parent-lifetime pipe, then fall back to
/// terminating this exact child if graceful cleanup does not finish in time.
pub(super) fn stop_owned_child(
    child: &mut std::process::Child,
    graceful_timeout: Duration,
) -> std::io::Result<std::process::ExitStatus> {
    child.stdin.take();
    let deadline = Instant::now() + graceful_timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child.kill()?;
            return child.wait();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
