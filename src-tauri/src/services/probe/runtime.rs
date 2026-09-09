//! Manual probe execution resources belong to one client instance.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::client_core::ProbeJobId;

pub const MAX_CONCURRENT_PROBES: usize = 8;

pub(crate) struct ProbeRuntime {
    pub(crate) jobs: Mutex<znet_client_core::probe_jobs::ProbeJobs>,
    controls: Mutex<HashMap<ProbeJobId, Control>>,
    concurrency: Arc<tokio::sync::Semaphore>,
    operations: Mutex<HashMap<(ProbeJobId, String), String>>,
}

impl Default for ProbeRuntime {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(Default::default()),
            controls: Mutex::new(HashMap::new()),
            concurrency: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PROBES)),
            operations: Mutex::new(HashMap::new()),
        }
    }
}

impl ProbeRuntime {
    #[cfg(test)]
    pub(crate) fn semaphore(&self) -> Arc<tokio::sync::Semaphore> {
        self.concurrency.clone()
    }

    pub(crate) fn remember(&self, job: ProbeJobId, target: &str, operation: String) {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .insert((job, target.to_owned()), operation);
    }

    /// A completion/core loss may arrive before the command acknowledgement.
    /// Never resurrect an operation that has already retired.
    pub(crate) fn acknowledge(&self, job: ProbeJobId, target: &str, operation: String) {
        if let Some(current) = self
            .operations
            .lock()
            .expect("probe operations poisoned")
            .get_mut(&(job, target.to_owned()))
        {
            *current = operation;
        }
    }

    pub(crate) fn expected(&self, job: ProbeJobId, target: &str) -> Option<String> {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .get(&(job, target.to_owned()))
            .cloned()
    }

    pub(crate) fn forget(&self, job: ProbeJobId, target: &str) {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .remove(&(job, target.to_owned()));
    }

    #[cfg(test)]
    pub(crate) fn forget_job(&self, job: ProbeJobId) {
        self.operations
            .lock()
            .expect("probe operations poisoned")
            .retain(|(id, _), _| *id != job);
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

const MAX_PENDING_TARGETS: usize = 256;
struct Control {
    cancelled: tokio::sync::watch::Sender<bool>,
    targets: HashMap<String, bool>, // false queued, true in flight
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRuntimeSnapshot {
    max_concurrency: usize,
    max_pending_targets: usize,
    queued: usize,
    in_flight: usize,
    draining: usize,
    jobs: usize,
    kernel_cancellation: &'static str,
}
impl ProbeRuntime {
    pub(crate) fn start(
        &self,
        scope: &crate::client_core::ClientScope,
        request: crate::client_core::StartProbeRequest,
        now: u64,
    ) -> Result<crate::client_core::StartProbeOutcome, crate::client_core::ClientCoreError> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let mut controls = self.controls.lock().unwrap_or_else(|e| e.into_inner());
        let pending: usize = controls.values().map(|v| v.targets.len()).sum();
        // Idempotent repeats must work even when the queue is full.
        let mut seen = std::collections::HashSet::new();
        let tags: Vec<_> = request
            .target_tags
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && seen.insert(*s))
            .collect();
        let existing = jobs.list_probe_jobs(None).iter().any(|j| {
            !j.state.is_terminal()
                && &j.scope == scope
                && j.kind == request.kind
                && j.target_tags
                    .iter()
                    .map(String::as_str)
                    .eq(tags.iter().copied())
        });
        if !existing && pending.saturating_add(tags.len()) > MAX_PENDING_TARGETS {
            return Err(crate::client_core::ClientCoreError {
                code: "probe_queue_full".into(),
                message: format!(
                    "测速队列已满，最多保留 {MAX_PENDING_TARGETS} 个目标；请等待或取消排队任务"
                ),
            });
        }
        let outcome = jobs.start_probe(scope, request, now)?;
        if outcome.created {
            controls.insert(
                outcome.job.id,
                Control {
                    cancelled: tokio::sync::watch::channel(false).0,
                    targets: outcome
                        .job
                        .target_tags
                        .iter()
                        .cloned()
                        .map(|s| (s, false))
                        .collect(),
                },
            );
        }
        Ok(outcome)
    }
    pub(crate) fn cancel_execution(&self, id: ProbeJobId) {
        let mut controls = self.controls.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(control) = controls.get_mut(&id) {
            control.cancelled.send_replace(true);
            control.targets.retain(|_, running| *running);
            if control.targets.is_empty() {
                controls.remove(&id);
            }
        }
    }
    pub(crate) fn invalidate(&self, reason: crate::client_core::ProbeJobState, now: u64) {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate_active_jobs(reason, now);
        let ids: Vec<_> = self
            .controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .copied()
            .collect();
        for id in ids {
            self.cancel_execution(id);
        }
        if reason == crate::client_core::ProbeJobState::InvalidatedByCoreRestart {
            self.operations
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clear();
        }
    }
    pub(crate) fn cancellation(
        &self,
        id: ProbeJobId,
    ) -> Option<tokio::sync::watch::Receiver<bool>> {
        self.controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .map(|v| v.cancelled.subscribe())
    }
    pub(crate) async fn acquire(&self, id: ProbeJobId, target: &str) -> Option<Lease<'_>> {
        let mut lease = Lease {
            owner: self,
            id,
            target: target.to_owned(),
            permit: None,
        };
        let mut cancelled = self.cancellation(id)?;
        if *cancelled.borrow() {
            return None;
        }
        let permit = tokio::select! {
            biased;
            _=cancelled.changed()=>return None,
            permit=self.concurrency.clone().acquire_owned()=>permit.ok()?,
        };
        let mut controls = self.controls.lock().unwrap_or_else(|e| e.into_inner());
        let control = controls.get_mut(&id)?;
        if *control.cancelled.borrow() {
            return None;
        }
        *control.targets.get_mut(target)? = true;
        drop(controls);
        lease.permit = Some(permit);
        Some(lease)
    }
    pub(crate) fn snapshot(&self) -> ProbeRuntimeSnapshot {
        let controls = self.controls.lock().unwrap_or_else(|e| e.into_inner());
        let mut snapshot = ProbeRuntimeSnapshot {
            max_concurrency: MAX_CONCURRENT_PROBES,
            max_pending_targets: MAX_PENDING_TARGETS,
            queued: 0,
            in_flight: 0,
            draining: 0,
            jobs: controls.len(),
            kernel_cancellation: "unsupported",
        };
        for control in controls.values() {
            for running in control.targets.values() {
                if *running {
                    snapshot.in_flight += 1;
                    if *control.cancelled.borrow() {
                        snapshot.draining += 1;
                    }
                } else if !*control.cancelled.borrow() {
                    snapshot.queued += 1;
                }
            }
        }
        snapshot
    }
}
/// Submitted work retains its permit until the response/deadline. Dropping a
/// UI subscription or cancelling a job cannot free capacity for more kernel work.
pub(crate) struct Lease<'a> {
    owner: &'a ProbeRuntime,
    id: ProbeJobId,
    target: String,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut controls = self
            .owner
            .controls
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(control) = controls.get_mut(&self.id) {
            control.targets.remove(&self.target);
            if control.targets.is_empty() {
                controls.remove(&self.id);
            }
        }
        drop(controls);
        self.owner.forget(self.id, &self.target);
        self.permit.take();
    }
}
