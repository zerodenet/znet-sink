use std::collections::{BTreeMap, HashMap};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use serde::Serialize;
use serde_json::Value;

use crate::client_core::ClientScope;
use crate::errors::{AppError, AppResult};
use crate::models::tool_job::{
    StartToolJobRequest, ToolJobError, ToolJobId, ToolJobKind, ToolJobSnapshot, ToolJobState,
};

const MAX_PENDING_JOBS: usize = 64;
const MAX_RETAINED_JOBS: usize = 128;
const DNS_CONCURRENCY: usize = 4;
const ROUTE_CONCURRENCY: usize = 2;
const MUTATION_CONCURRENCY: usize = 1;
const MIN_TIMEOUT_MS: u64 = 1_000;
const MAX_TIMEOUT_MS: u64 = 120_000;

#[derive(Clone, Copy)]
enum Budget {
    Dns,
    Route,
    Mutation,
}

struct Control {
    cancel: tokio::sync::watch::Sender<bool>,
    budget: Budget,
}

pub(crate) struct ToolRuntime {
    jobs: Mutex<BTreeMap<ToolJobId, ToolJobSnapshot>>,
    controls: Mutex<HashMap<ToolJobId, Control>>,
    next_id: AtomicU64,
    dns: Arc<tokio::sync::Semaphore>,
    route: Arc<tokio::sync::Semaphore>,
    mutation: Arc<tokio::sync::Semaphore>,
}

impl Default for ToolRuntime {
    fn default() -> Self {
        Self {
            jobs: Mutex::new(BTreeMap::new()),
            controls: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            dns: Arc::new(tokio::sync::Semaphore::new(DNS_CONCURRENCY)),
            route: Arc::new(tokio::sync::Semaphore::new(ROUTE_CONCURRENCY)),
            mutation: Arc::new(tokio::sync::Semaphore::new(MUTATION_CONCURRENCY)),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRuntimeSnapshot {
    max_pending_jobs: usize,
    dns_max_concurrency: usize,
    route_max_concurrency: usize,
    mutation_max_concurrency: usize,
    queued: usize,
    running: usize,
    cancelling: usize,
    retained: usize,
    active: usize,
    kernel_cancellation: &'static str,
    kinds: Vec<ToolKindRuntimeSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolKindRuntimeSnapshot {
    kind: ToolJobKind,
    queued: usize,
    running: usize,
    cancelling: usize,
    retained: usize,
}

impl ToolRuntime {
    pub(crate) fn start(
        &self,
        scope: ClientScope,
        request: StartToolJobRequest,
        now: u64,
    ) -> AppResult<ToolJobSnapshot> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let active = jobs.values().filter(|job| !job.state.is_terminal()).count();
        if active >= MAX_PENDING_JOBS {
            return Err(AppError::conflict(
                "tool_jobs",
                "capacity",
                format!("诊断任务队列已满，最多保留 {MAX_PENDING_JOBS} 个活动任务"),
            ));
        }

        let id = ToolJobId(self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let timeout_ms = request
            .timeout_ms
            .unwrap_or_else(|| default_timeout(request.kind))
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
        let params = normalize_params(request.params)?;
        let job = ToolJobSnapshot {
            id,
            scope,
            kind: request.kind,
            state: ToolJobState::Queued,
            subject: subject(request.kind, &params),
            params,
            result: None,
            error: None,
            created_at_unix_ms: now,
            started_at_unix_ms: None,
            updated_at_unix_ms: now,
            deadline_at_unix_ms: now.saturating_add(timeout_ms),
        };
        jobs.insert(id, job.clone());
        prune(&mut jobs);
        drop(jobs);
        self.controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                Control {
                    cancel: tokio::sync::watch::channel(false).0,
                    budget: budget(request.kind),
                },
            );
        Ok(job)
    }

    pub(crate) fn get(&self, id: ToolJobId) -> Option<ToolJobSnapshot> {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
    }

    pub(crate) fn list(&self, kind: Option<ToolJobKind>) -> Vec<ToolJobSnapshot> {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|job| kind.is_none_or(|kind| job.kind == kind))
            .cloned()
            .collect()
    }

    pub(crate) fn mark_running(&self, id: ToolJobId, now: u64) -> Option<ToolJobSnapshot> {
        self.update(id, |job| {
            if job.state != ToolJobState::Queued {
                return false;
            }
            job.state = ToolJobState::Running;
            job.started_at_unix_ms = Some(now);
            job.updated_at_unix_ms = now;
            true
        })
    }

    pub(crate) fn complete(
        &self,
        id: ToolJobId,
        result: AppResult<Value>,
        now: u64,
    ) -> Option<ToolJobSnapshot> {
        let update = self.update(id, |job| {
            if job.state.is_terminal() || job.state == ToolJobState::Cancelling {
                return false;
            }
            match result.clone() {
                Ok(value) => {
                    job.state = ToolJobState::Completed;
                    job.result = Some(value);
                    job.error = None;
                }
                Err(error) => {
                    job.state = ToolJobState::Failed;
                    job.error = Some(error.into());
                }
            }
            job.updated_at_unix_ms = now;
            true
        });
        self.controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        update.or_else(|| self.get(id))
    }

    pub(crate) fn timeout(&self, id: ToolJobId, now: u64) -> Option<ToolJobSnapshot> {
        self.signal(id);
        let update = self.finish(id, ToolJobState::TimedOut, now);
        self.controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        update.or_else(|| self.get(id))
    }

    pub(crate) fn cancel(&self, id: ToolJobId, now: u64) -> Option<ToolJobSnapshot> {
        let update = self.update(id, |job| {
            if job.state.is_terminal() {
                return false;
            }
            job.state = if job.state == ToolJobState::Queued {
                ToolJobState::Cancelled
            } else {
                ToolJobState::Cancelling
            };
            job.updated_at_unix_ms = now;
            true
        });
        self.signal(id);
        update.or_else(|| self.get(id))
    }

    pub(crate) fn finish_cancel(&self, id: ToolJobId, now: u64) -> Option<ToolJobSnapshot> {
        let update = self.finish(id, ToolJobState::Cancelled, now);
        self.controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        update.or_else(|| self.get(id))
    }

    pub(crate) fn invalidate(&self, state: ToolJobState, now: u64) {
        debug_assert!(matches!(
            state,
            ToolJobState::InvalidatedByConfigChange | ToolJobState::InvalidatedByCoreRestart
        ));
        let ids = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
            jobs.values_mut()
                .filter(|job| !job.state.is_terminal())
                .map(|job| {
                    job.state = state;
                    job.updated_at_unix_ms = now;
                    job.id
                })
                .collect::<Vec<_>>()
        };
        for id in ids {
            self.signal(id);
        }
    }

    pub(crate) async fn acquire(&self, id: ToolJobId) -> Option<tokio::sync::OwnedSemaphorePermit> {
        let (mut cancelled, semaphore) = {
            let controls = self.controls.lock().unwrap_or_else(|e| e.into_inner());
            let control = controls.get(&id)?;
            let semaphore = match control.budget {
                Budget::Dns => self.dns.clone(),
                Budget::Route => self.route.clone(),
                Budget::Mutation => self.mutation.clone(),
            };
            (control.cancel.subscribe(), semaphore)
        };
        if *cancelled.borrow() {
            return None;
        }
        tokio::select! {
            biased;
            _ = cancelled.changed() => None,
            permit = semaphore.acquire_owned() => permit.ok(),
        }
    }

    pub(crate) fn cancellation(&self, id: ToolJobId) -> Option<tokio::sync::watch::Receiver<bool>> {
        self.controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .map(|control| control.cancel.subscribe())
    }

    pub(crate) fn snapshot(&self) -> ToolRuntimeSnapshot {
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let kinds = [
            ToolJobKind::DnsLookup,
            ToolJobKind::DnsCache,
            ToolJobKind::FakeIpLookup,
            ToolJobKind::FakeIpClear,
            ToolJobKind::RouteTrace,
        ]
        .into_iter()
        .map(|kind| ToolKindRuntimeSnapshot {
            kind,
            queued: jobs
                .values()
                .filter(|job| job.kind == kind && job.state == ToolJobState::Queued)
                .count(),
            running: jobs
                .values()
                .filter(|job| job.kind == kind && job.state == ToolJobState::Running)
                .count(),
            cancelling: jobs
                .values()
                .filter(|job| job.kind == kind && job.state == ToolJobState::Cancelling)
                .count(),
            retained: jobs.values().filter(|job| job.kind == kind).count(),
        })
        .collect();
        ToolRuntimeSnapshot {
            max_pending_jobs: MAX_PENDING_JOBS,
            dns_max_concurrency: DNS_CONCURRENCY,
            route_max_concurrency: ROUTE_CONCURRENCY,
            mutation_max_concurrency: MUTATION_CONCURRENCY,
            queued: jobs
                .values()
                .filter(|job| job.state == ToolJobState::Queued)
                .count(),
            running: jobs
                .values()
                .filter(|job| job.state == ToolJobState::Running)
                .count(),
            cancelling: jobs
                .values()
                .filter(|job| job.state == ToolJobState::Cancelling)
                .count(),
            retained: jobs.len(),
            active: jobs.values().filter(|job| !job.state.is_terminal()).count(),
            kernel_cancellation: "queued requests cancel immediately; submitted requests drain before cancellation completes",
            kinds,
        }
    }

    fn finish(&self, id: ToolJobId, state: ToolJobState, now: u64) -> Option<ToolJobSnapshot> {
        self.update(id, |job| {
            if job.state.is_terminal() {
                return false;
            }
            job.state = state;
            job.updated_at_unix_ms = now;
            true
        })
    }

    fn signal(&self, id: ToolJobId) {
        if let Some(control) = self
            .controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
        {
            control.cancel.send_replace(true);
        }
    }

    fn update(
        &self,
        id: ToolJobId,
        update: impl FnOnce(&mut ToolJobSnapshot) -> bool,
    ) -> Option<ToolJobSnapshot> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let job = jobs.get_mut(&id)?;
        update(job).then(|| job.clone())
    }
}

impl From<AppError> for ToolJobError {
    fn from(error: AppError) -> Self {
        Self {
            code: error.code.to_owned(),
            message: error.message,
            details: error.details,
        }
    }
}

fn normalize_params(params: Value) -> AppResult<Value> {
    if params.is_null() {
        return Ok(serde_json::json!({}));
    }
    if !params.is_object() {
        return Err(AppError::invalid_argument(
            "tool job params must be an object",
        ));
    }
    Ok(params)
}

fn default_timeout(kind: ToolJobKind) -> u64 {
    match kind {
        ToolJobKind::RouteTrace => 60_000,
        ToolJobKind::DnsLookup | ToolJobKind::DnsCache | ToolJobKind::FakeIpLookup => 20_000,
        ToolJobKind::FakeIpClear => 30_000,
    }
}

fn budget(kind: ToolJobKind) -> Budget {
    match kind {
        ToolJobKind::RouteTrace => Budget::Route,
        ToolJobKind::FakeIpClear => Budget::Mutation,
        ToolJobKind::DnsLookup | ToolJobKind::DnsCache | ToolJobKind::FakeIpLookup => Budget::Dns,
    }
}

fn subject(kind: ToolJobKind, params: &Value) -> String {
    let field = match kind {
        ToolJobKind::DnsLookup => "hostname",
        ToolJobKind::DnsCache => "domain",
        ToolJobKind::FakeIpLookup | ToolJobKind::FakeIpClear => {
            if params.get("domain").is_some() {
                "domain"
            } else {
                "ip"
            }
        }
        ToolJobKind::RouteTrace => "target",
    };
    params
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if kind == ToolJobKind::FakeIpClear {
                "全部映射"
            } else {
                "未指定"
            }
        })
        .to_owned()
}

fn prune(jobs: &mut BTreeMap<ToolJobId, ToolJobSnapshot>) {
    if jobs.len() <= MAX_RETAINED_JOBS {
        return;
    }
    let remove = jobs
        .iter()
        .filter_map(|(id, job)| job.state.is_terminal().then_some(*id))
        .take(jobs.len() - MAX_RETAINED_JOBS)
        .collect::<Vec<_>>();
    for id in remove {
        jobs.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_core::{ConfigRevision, CoreInstanceId, ProfileId};

    fn scope() -> ClientScope {
        ClientScope {
            profile_id: Some(ProfileId("profile-a".into())),
            config_revision: ConfigRevision(2),
            core_instance_id: CoreInstanceId(3),
        }
    }

    fn request(kind: ToolJobKind) -> StartToolJobRequest {
        StartToolJobRequest {
            kind,
            params: serde_json::json!({"target":"example.test"}),
            timeout_ms: Some(5_000),
        }
    }

    #[test]
    fn queued_and_running_jobs_have_explicit_cancel_states() {
        let runtime = ToolRuntime::default();
        let queued = runtime
            .start(scope(), request(ToolJobKind::RouteTrace), 100)
            .unwrap();
        assert_eq!(
            runtime.cancel(queued.id, 101).unwrap().state,
            ToolJobState::Cancelled
        );

        let running = runtime
            .start(scope(), request(ToolJobKind::RouteTrace), 200)
            .unwrap();
        assert_eq!(
            runtime.mark_running(running.id, 201).unwrap().state,
            ToolJobState::Running
        );
        assert_eq!(
            runtime.cancel(running.id, 202).unwrap().state,
            ToolJobState::Cancelling
        );
        assert_eq!(
            runtime.finish_cancel(running.id, 203).unwrap().state,
            ToolJobState::Cancelled
        );
    }

    #[test]
    fn configuration_and_core_transitions_invalidate_recoverable_snapshots() {
        let runtime = ToolRuntime::default();
        let config = runtime
            .start(scope(), request(ToolJobKind::DnsLookup), 100)
            .unwrap();
        runtime.invalidate(ToolJobState::InvalidatedByConfigChange, 101);
        assert_eq!(
            runtime.get(config.id).unwrap().state,
            ToolJobState::InvalidatedByConfigChange
        );

        let core = runtime
            .start(scope(), request(ToolJobKind::RouteTrace), 200)
            .unwrap();
        runtime.invalidate(ToolJobState::InvalidatedByCoreRestart, 201);
        assert_eq!(
            runtime.get(core.id).unwrap().state,
            ToolJobState::InvalidatedByCoreRestart
        );
    }

    #[tokio::test]
    async fn dns_and_route_use_independent_concurrency_budgets() {
        let runtime = ToolRuntime::default();
        let dns = runtime
            .start(scope(), request(ToolJobKind::DnsLookup), 100)
            .unwrap();
        let route = runtime
            .start(scope(), request(ToolJobKind::RouteTrace), 100)
            .unwrap();
        let dns_permit = runtime.acquire(dns.id).await.unwrap();
        let route_permit = runtime.acquire(route.id).await.unwrap();
        assert_eq!(runtime.dns.available_permits(), DNS_CONCURRENCY - 1);
        assert_eq!(runtime.route.available_permits(), ROUTE_CONCURRENCY - 1);
        drop((dns_permit, route_permit));
    }
}
