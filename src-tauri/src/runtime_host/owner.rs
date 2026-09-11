use crate::models::core_process::{CoreProcessState, CoreProcessStatus};
use std::{process::Child, thread::JoinHandle};
pub(super) struct ManagedCoreProcess {
    pub child: Option<Child>,
    pub stderr_handle: Option<JoinHandle<()>>,
    pub status: CoreProcessStatus,
    pub endpoint: Option<crate::models::core::CoreEndpoint>,
}

impl Drop for ManagedCoreProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            eprintln!(
                "[ZNet] shutdown: closing core lifetime pipe (pid={})",
                child.id()
            );
            child.stdin.take();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    _ => {
                        // This is still scoped to the exact child owned by
                        // this state; never kill processes by executable name.
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        self.stderr_handle.take().map(|h| h.join());
    }
}

impl Default for ManagedCoreProcess {
    fn default() -> Self {
        Self {
            child: None,
            endpoint: None,
            stderr_handle: None,
            status: CoreProcessStatus {
                state: CoreProcessState::NotStarted,
                pid: None,
                kernel: "zero".to_string(),
                executable_path: None,
                working_dir: None,
                config_path: None,
                endpoint_path: String::new(),
                started_at_unix_ms: None,
                exited_at_unix_ms: None,
                exit_code: None,
                exit_reason: None,
                last_error: None,
            },
        }
    }
}

use crate::errors::{AppError, AppResult};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, MutexGuard,
};

#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSnapshot {
    pub desired_running: bool,
    pub operation: Option<String>,
    pub generation: u64,
    pub last_error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Host {
    process: Mutex<ManagedCoreProcess>,
    transition: Mutex<()>,
    generation: AtomicU64,
    diagnostics: Mutex<HostSnapshot>,
}
impl Host {
    pub(super) fn process(&self) -> &Mutex<ManagedCoreProcess> {
        &self.process
    }
    pub(crate) fn process_status(&self) -> AppResult<CoreProcessStatus> {
        self.process
            .lock()
            .map(|process| process.status.clone())
            .map_err(|_| AppError::internal("managed process state lock poisoned"))
    }
    pub(crate) fn try_process_status(&self) -> Option<CoreProcessStatus> {
        self.process
            .try_lock()
            .ok()
            .map(|process| process.status.clone())
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    pub(crate) fn advance_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }
    pub(crate) fn snapshot(&self) -> HostSnapshot {
        let mut snapshot = self
            .diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        snapshot.generation = self.generation();
        snapshot
    }
    pub(super) fn run<T>(
        &self,
        name: &str,
        desired: bool,
        action: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        let guard = self.transition.lock().unwrap_or_else(|e| e.into_inner());
        let mut operation = self.begin(guard, name, Some(desired));
        let result = action();
        operation.finish(&result);
        result
    }
    pub(super) fn monitor_operation(&self, generation: u64) -> Option<Operation<'_>> {
        let guard = self.transition.try_lock().ok()?;
        if self.generation() != generation || !self.snapshot().desired_running {
            return None;
        }
        Some(self.begin(guard, "monitor", None))
    }
    fn begin<'a>(
        &'a self,
        guard: MutexGuard<'a, ()>,
        name: &str,
        desired: Option<bool>,
    ) -> Operation<'a> {
        let mut diagnostic = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(desired) = desired {
            diagnostic.desired_running = desired;
        }
        diagnostic.operation = Some(name.into());
        Operation {
            host: self,
            _guard: guard,
            finished: false,
        }
    }
    pub(crate) fn record_error(&self, error: impl Into<String>) {
        self.diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last_error = Some(error.into());
    }
}

pub(super) struct Operation<'a> {
    host: &'a Host,
    _guard: MutexGuard<'a, ()>,
    finished: bool,
}
impl Operation<'_> {
    pub(super) fn finish<T>(&mut self, result: &Result<T, AppError>) {
        if let Err(error) = result {
            self.host.record_error(error.message.clone());
        }
        self.finished = true;
    }
}
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        let mut diagnostic = self
            .host
            .diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        diagnostic.operation = None;
        if !self.finished && std::thread::panicking() {
            diagnostic.last_error = Some("runtime operation panicked".into());
        }
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
