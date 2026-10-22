//! A bounded, headless OS execution owner over the existing plugin queue.
//! This is not a second task registry and never creates durable authority.
use super::scheduler::attempt::Completion;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Outcome {
    pub state: &'static str,
    pub completed: u32,
    pub failed: u32,
    pub unconfirmed: u32,
    pub reason: Option<&'static str>,
}
impl Outcome {
    pub(crate) fn deferred(reason: &'static str) -> Self {
        Self {
            state: "deferred",
            reason: Some(reason),
            ..Self::default()
        }
    }
}

/// Seals the exact OS lease on success, early error and unwind. Cleanup must
/// never close an executor subsequently acquired by a foreground lifetime.
struct RunOwner<'a> {
    host: &'a Host,
    permit: platform::BackgroundPermit,
}
impl Drop for RunOwner<'_> {
    fn drop(&mut self) {
        let _operation = self
            .host
            .operation
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.host.platform.end_background(&self.permit) {
            self.host
                .executor_shutdown_handle()
                .close(std::time::Duration::ZERO);
        }
    }
}

pub(crate) fn run(
    state: crate::state::app_state::AppState,
    target: Target,
    reader: vault::KeyReader,
    cancelled: Arc<AtomicBool>,
) -> AppResult<Outcome> {
    let host = state.plugins();
    let permit = {
        let _operation = host.operation.lock().map_err(|_| stale())?;
        let permit = match host
            .platform
            .begin_background(target, true, cancelled.clone())
        {
            Ok(permit) => permit,
            Err(_) => return Ok(Outcome::deferred("foreground-or-other-owner")),
        };
        permit
    };
    let _owner = RunOwner {
        host,
        permit: permit.clone(),
    };
    {
        let _operation = host.operation.lock().map_err(|_| stale())?;
        permit.check()?;
        *host.native_vault.lock().map_err(|_| stale())? = Some(reader);
        let mut executor = host.executor.lock().map_err(|_| stale())?;
        if !executor.snapshot().accepting {
            *executor = Default::default();
        }
    }
    // Even failure must invalidate this job's admission, without closing a
    // newer foreground executor that may already have resumed.
    (|| {
        let _admission = permit.enter()?;
        let root = host.root()?;
        std::fs::create_dir_all(&root).map_err(io::failure)?;
        host.restore_cached(state.capabilities());
        if host.state.lock().map_err(|_| stale())?.directory.is_none() {
            return Ok(Outcome::deferred("no-verified-plugin-catalog"));
        }
        {
            let _operation = host.operation_guard()?;
            host.checked_rescan(state.capabilities())?;
        }
        let due = host.take_due_limited(
            state.capabilities(),
            crate::services::common::now_unix_ms(),
            8,
        )?;
        if due.is_empty() {
            return Ok(Outcome::deferred("no-enabled-due-tasks"));
        }
        let mut outcome = Outcome {
            state: "completed",
            ..Outcome::default()
        };
        for due in due {
            if permit.check().is_err() {
                outcome.state = "stopped";
                break;
            }
            let result = host.invoke_scheduled_with_context(
                state.clone(),
                None,
                &due,
                cancelled.clone(),
                Some(permit.clone()),
            );
            let declared_failure = result.as_ref().ok().is_some_and(|value| {
                value.get("ok").and_then(serde_json::Value::as_bool) == Some(false)
            });
            let completion = if result.is_err() {
                Completion::Unconfirmed
            } else {
                Completion::Known(!declared_failure)
            };
            // A stopped/invalidated run may not publish into a replacement
            // scope. Its durable running checkpoint recovers as unconfirmed.
            let recorded = host.record_scheduled_completion(
                state.capabilities(),
                &due,
                completion,
                crate::services::common::now_unix_ms(),
            );
            if result.is_err() || recorded.is_err() {
                outcome.unconfirmed += 1;
            } else if declared_failure {
                outcome.failed += 1;
            } else {
                outcome.completed += 1;
            }
            let retry_error = AppError::invalid_argument("插件请求后台任务重试");
            let execution_result = if declared_failure {
                Err(&retry_error)
            } else {
                result.as_ref().map(|_| ())
            };
            let audit_result = recorded.as_ref().map(|_| ()).and(execution_result);
            crate::services::logs::plugin_host_event(
                &state,
                &due.task.plugin_id,
                &due.task.component_id,
                crate::models::logs::PluginAction::ScheduledComponentAction,
                audit_result,
            );
            if cancelled.load(Ordering::Acquire) || permit.check().is_err() {
                outcome.state = "stopped";
                break;
            }
        }
        if outcome.unconfirmed > 0 {
            outcome.reason = Some("unconfirmed-not-replayed");
        }
        Ok(outcome)
    })()
}

#[cfg(test)]
mod tests {
    use super::*;
    use znet_plugin_sandbox::contract::{Arch, DeviceClass, Os};
    fn phone() -> Target {
        Target {
            os: Os::Android,
            arch: Arch::Aarch64,
            device: DeviceClass::Phone,
        }
    }
    fn host() -> Host {
        Host {
            platform: platform::android_context_for_test(),
            ..Host::default()
        }
    }
    #[test]
    fn unwinding_run_releases_only_original_os_owner() {
        let host = host();
        let cancelled = Arc::new(AtomicBool::new(false));
        let permit = host
            .platform
            .begin_background(phone(), false, cancelled.clone())
            .unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _owner = RunOwner {
                host: &host,
                permit,
            };
            panic!("isolated test unwind");
        }));
        assert!(outcome.is_err());
        assert!(cancelled.load(Ordering::Acquire));
        let next = host
            .platform
            .begin_background(phone(), false, Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert!(host.platform.end_background(&next));
    }
    #[test]
    fn setup_lock_failure_does_not_leave_background_owner_stuck() {
        let host = host();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = host.native_vault.lock().unwrap();
            panic!("poison vault fixture");
        }));
        let state =
            crate::state::app_state::AppState::with_plugin_fixture(host, Manager::default());
        let result = run(
            state.clone(),
            phone(),
            Arc::new(|_| Ok(None)),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(result.is_err());
        let next = state
            .plugins()
            .platform
            .begin_background(phone(), false, Arc::new(AtomicBool::new(false)))
            .unwrap();
        assert!(state.plugins().platform.end_background(&next));
    }
}
