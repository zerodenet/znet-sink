use crate::errors::AppResult;
use crate::models::{
    core_process::{CoreProcessExitReason, CoreProcessState, CoreProcessStatus},
    logs::{LogLevel, LogSource},
};
use crate::services::{common::lock, logs, system_proxy_guard};
use crate::state::app_state::AppState;

pub(crate) fn refresh_status(state: &AppState) -> AppResult<CoreProcessStatus> {
    let process = lock(state.runtime_host().process(), "core_process")?;
    // Lifecycle mutation belongs to the host; status callers only project it.
    // The watchdog refreshes exits under the shared operation guard.
    let status = process.status.clone();
    drop(process);
    let source_status = match status.state {
        CoreProcessState::Running => crate::client_core::SourceStatus::Ready,
        CoreProcessState::Starting => crate::client_core::SourceStatus::Initializing,
        CoreProcessState::Failed => crate::client_core::SourceStatus::Degraded,
        CoreProcessState::NotStarted | CoreProcessState::Exited => {
            crate::client_core::SourceStatus::Offline
        }
    };
    state.set_client_core_source_status(source_status);
    Ok(status)
}

pub(super) fn refresh_locked_status(
    process: &mut super::ManagedCoreProcess,
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

            if let Some(endpoint) = process.endpoint.take() {
                crate::kernel::connection::reset_endpoint(&endpoint);
            }
            if let Err(error) = system_proxy_guard::disable_with_guard() {
                state
                    .runtime_host()
                    .record_error(format!("proxy cleanup after core exit: {}", error.message));
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
