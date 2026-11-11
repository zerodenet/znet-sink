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

#[derive(Clone, Copy)]
enum StopIntent {
    Requested,
    Recovery,
}

impl StopIntent {
    fn exit_reason(self) -> CoreProcessExitReason {
        match self {
            Self::Requested => CoreProcessExitReason::Stopped,
            Self::Recovery => CoreProcessExitReason::Crashed,
        }
    }
}

pub fn stop(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<CoreProcessStatus> {
    state.runtime_host().run("stop", false, || {
        stop_with_proxy_cleanup(app_handle.clone(), state.clone(), true)
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
        stop_with_proxy_cleanup(app_handle.clone(), state.clone(), false)
    })
}

pub(super) fn stop_with_proxy_cleanup(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    cleanup_system_proxy: bool,
) -> AppResult<CoreProcessStatus> {
    stop_with_cleanup(
        app_handle,
        state,
        cleanup_system_proxy,
        None,
        StopIntent::Requested,
    )
}

/// The caller owns the monitor operation. Keep that generation alive, but use
/// exactly the same TUN cleanup and child-exit confirmation as a requested stop.
pub(super) fn retire_for_recovery(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<CoreProcessStatus> {
    stop_with_cleanup(app_handle, state, true, None, StopIntent::Recovery)
}

pub(super) fn stop_for_exit(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    cleanup_system_proxy: bool,
) -> AppResult<CoreProcessStatus> {
    // Fallback authority for an older installation without a proxy marker:
    // only a live owned child with a recorded private control endpoint qualifies.
    let has_owned_child = {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        let alive = process
            .child
            .as_mut()
            .is_some_and(|child| child.try_wait().is_ok_and(|exit| exit.is_none()));
        alive && process.endpoint.is_some()
    };
    let owned_listener = if has_owned_child {
        crate::configuration::preferences::endpoint(state.inner()).ok()
    } else {
        None
    };
    stop_with_cleanup(
        app_handle,
        state,
        cleanup_system_proxy,
        owned_listener,
        StopIntent::Requested,
    )
}

fn stop_with_cleanup(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    cleanup_system_proxy: bool,
    owned_listener: Option<(String, u16)>,
    intent: StopIntent,
) -> AppResult<CoreProcessStatus> {
    // Cancel the old watchdog even when the child already exited or is in backoff.
    if matches!(intent, StopIntent::Requested) {
        state.next_core_process_monitor_generation();
    }
    // Keep cleanup available even without a child, and do not abandon it if
    // TUN retirement fails. TUN must get its stop attempt before capture is
    // relinquished; DNS/proxy failures cannot prevent child-exit confirmation.
    let release_capture = || {
        super::cleanup::release_capture(cleanup_system_proxy, crate::capture::dns::release, || {
            system_proxy_guard::clear_on_exit(owned_listener)
        })
    };
    let (child, mut stderr_handle, endpoint, working_dir) = {
        let mut process = lock(state.runtime_host().process(), "core_process")?;
        if let Err(error) = refresh_locked_status(&mut process, state.inner()) {
            // Keep the original child for the retirement attempt below. A
            // failed poll must not skip TUN, DNS or proxy cleanup.
            state.runtime_host().record_error(error.message);
        }
        if process.child.is_some() {
            // Mark the stop intent before releasing the child handle.
            // The watchdog polls `child.is_none()` once per second; without
            // this early marker it can see "no child + no Stopped reason"
            // in the small window before we finish kill/wait bookkeeping and
            // misclassify a user-initiated stop/restart as a crash.
            process.status.exit_reason = Some(intent.exit_reason());
            process.status.last_error = None;
        }
        process.status.exit_reason = Some(intent.exit_reason());
        (
            process.child.take(),
            process.stderr_handle.take(),
            process.endpoint.clone(),
            process.status.working_dir.clone(),
        )
    };

    let Some(mut child) = child else {
        // The GUI only controls children it owns. An independently launched
        // Zero process is deliberately outside this lifecycle.
        release_capture()?;
        let tun = lock(state.app_config(), "app_config")?.tun.clone();
        crate::capture::residue::confirm_absent(&tun, working_dir.as_deref())?;
        let status = refresh_status(state.inner());
        return status;
    };

    let pid = child.id();
    let retirement = super::retire::retire_owned_child(
        &mut child,
        endpoint.as_ref().map(|endpoint| endpoint.path.as_str()),
    );
    let native_audit = if retirement.exit.is_ok() {
        let tun = lock(state.app_config(), "app_config")?.tun.clone();
        crate::capture::residue::confirm_absent(&tun, working_dir.as_deref())
    } else {
        Ok(())
    };
    let tun_cleanup = match (retirement.tun_cleanup, native_audit) {
        (Ok(()), result) => result,
        (result, Ok(())) => result,
        (Err(tun), Err(native)) => Err(AppError {
            code: "tun_cleanup_unconfirmed",
            message: format!("{}; native cleanup audit: {}", tun.message, native.message),
            details: None,
        }),
    };
    let capture_result = match (release_capture(), tun_cleanup) {
        (Ok(()), result) => result,
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(tun)) => Err(AppError::internal(format!(
            "{}; TUN cleanup failed: {}",
            error.message, tun.message
        ))),
    };
    let wait_result = retirement.exit;

    if wait_result.is_ok() {
        if let Some(handle) = stderr_handle.take() {
            let _ = handle.join();
        }
    }

    let mut process = lock(state.runtime_host().process(), "core_process")?;
    match wait_result {
        Ok(status) => {
            if let Some(endpoint) = &endpoint {
                crate::kernel::connection::reset_endpoint(endpoint);
            }
            process.endpoint = None;
            process.status.state = CoreProcessState::Exited;
            process.status.pid = None;
            process.status.exit_code = status.code();
            process.status.exit_reason = Some(intent.exit_reason());
            process.status.exited_at_unix_ms = Some(crate::services::common::now_unix_ms());
            process.status.last_error = capture_result
                .as_ref()
                .err()
                .map(|error| error.message.clone());

            let _ = logs::append_entry(
                state.inner(),
                LogSource::App,
                LogLevel::Info,
                "core process exit confirmed".to_string(),
                Some(json!({ "pid": pid, "exitCode": status.code() })),
            );

            let status = process.status.clone();
            drop(process);
            state.client_core_instance_lost();
            let _ = app_handle.emit(
                crate::services::probe::CLIENT_CORE_UPDATED_EVENT,
                state.client_core_snapshot(),
            );
            capture_result?;
            Ok(status)
        }
        Err(error) => {
            let mut message = format!("failed to stop core process: {error}");
            if let Err(cleanup) = capture_result {
                message.push_str(&format!("; {}", cleanup.message));
            }
            process.child = Some(child);
            process.stderr_handle = stderr_handle;
            process.status.pid = Some(pid);
            process.status.exited_at_unix_ms = None;
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
