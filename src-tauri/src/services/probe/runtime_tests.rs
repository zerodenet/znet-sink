use super::*;
use crate::state::app_state::AppState;

#[test]
fn exhausted_probe_budget_does_not_block_another_client_instance() {
    let first = AppState::default();
    let second = AppState::default();
    let pool = first.probe_runtime().semaphore();
    let permits = pool
        .clone()
        .try_acquire_many_owned(MAX_CONCURRENT_PROBES as u32)
        .unwrap();
    assert!(pool.clone().try_acquire_owned().is_err());
    assert!(second
        .probe_runtime()
        .semaphore()
        .try_acquire_owned()
        .is_ok());
    drop(permits);
    assert_eq!(pool.available_permits(), MAX_CONCURRENT_PROBES);
}

#[test]
fn identical_job_ids_and_cleanup_are_isolated_between_instances() {
    let first = AppState::default();
    let second = AppState::default();
    let job = ProbeJobId(1);
    first.probe_runtime().remember(job, "auto", "first".into());
    second
        .probe_runtime()
        .remember(job, "auto", "second".into());
    first.probe_runtime().forget_job(job);
    assert_eq!(first.probe_runtime().expected(job, "auto"), None);
    assert_eq!(
        second.probe_runtime().expected(job, "auto").as_deref(),
        Some("second")
    );
}

use crate::client_core::{
    ClientScope, ConfigRevision, CoreInstanceId, ProbeJobKind, ProbeJobState, ProfileId,
    StartProbeRequest,
};
fn scope() -> ClientScope {
    ClientScope {
        profile_id: Some(ProfileId("test".into())),
        config_revision: ConfigRevision(1),
        core_instance_id: CoreInstanceId(1),
    }
}
fn request(tags: Vec<String>) -> StartProbeRequest {
    StartProbeRequest {
        kind: ProbeJobKind::Outbound,
        target_tags: tags,
        timeout_ms: Some(30_000),
    }
}
fn start(runtime: &ProbeRuntime, tags: &[&str]) -> ProbeJobId {
    runtime
        .start(
            &scope(),
            request(tags.iter().map(|s| s.to_string()).collect()),
            1,
        )
        .unwrap()
        .job
        .id
}

#[tokio::test]
async fn cancelling_queued_work_wakes_waiters_without_releasing_submitted_capacity() {
    let runtime = ProbeRuntime::default();
    let id = start(&runtime, &["sent", "queued"]);
    let running = runtime.acquire(id, "sent").await.unwrap();
    let occupied = runtime.semaphore().try_acquire_many_owned(7).unwrap();
    let waiting = runtime.acquire(id, "queued");
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut waiting)
            .await
            .is_err()
    );
    runtime.cancel_execution(id);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut waiting)
            .await
            .unwrap()
            .is_none()
    );
    let snapshot = runtime.snapshot();
    assert_eq!(
        (snapshot.queued, snapshot.in_flight, snapshot.draining),
        (0, 1, 1)
    );
    assert_eq!(runtime.semaphore().available_permits(), 0);
    drop(running);
    assert_eq!(runtime.snapshot().jobs, 0);
    assert_eq!(runtime.semaphore().available_permits(), 1);
    drop(occupied);
}

#[test]
fn queue_admission_is_bounded_deduplicated_and_refilled_after_cancel() {
    let runtime = ProbeRuntime::default();
    let tags: Vec<_> = (0..MAX_PENDING_TARGETS)
        .map(|n| format!("node-{n}"))
        .collect();
    let first = runtime.start(&scope(), request(tags.clone()), 1).unwrap();
    let mut repeated = tags;
    repeated.push(" node-0 ".into());
    assert!(
        !runtime
            .start(&scope(), request(repeated), 2)
            .unwrap()
            .created
    );
    let error = runtime
        .start(&scope(), request(vec!["extra".into()]), 2)
        .unwrap_err();
    assert_eq!(error.code, "probe_queue_full");
    runtime.cancel_execution(first.job.id);
    runtime.jobs.lock().unwrap().cancel_probe(first.job.id, 3);
    assert!(
        runtime
            .start(&scope(), request(vec!["extra".into()]), 4)
            .unwrap()
            .created
    );
    assert_eq!(runtime.snapshot().queued, 1);
}

#[tokio::test]
async fn config_invalidation_stops_queue_but_preserves_inflight_operation_until_retired() {
    let runtime = ProbeRuntime::default();
    let id = start(&runtime, &["sent", "queued"]);
    let lease = runtime.acquire(id, "sent").await.unwrap();
    runtime.remember(id, "sent", "op".into());
    runtime.invalidate(ProbeJobState::InvalidatedByConfigChange, 10);
    assert!(runtime.acquire(id, "queued").await.is_none());
    assert_eq!(runtime.expected(id, "sent").as_deref(), Some("op"));
    assert_eq!(runtime.snapshot().draining, 1);
    drop(lease);
    assert_eq!(runtime.expected(id, "sent"), None);
    assert_eq!(runtime.snapshot().jobs, 0);
    assert_eq!(runtime.semaphore().available_permits(), 8);
}

#[tokio::test]
async fn dropping_waiter_reclaims_queue_and_core_loss_retires_policy_wait() {
    let runtime = ProbeRuntime::default();
    let id = start(&runtime, &["pending"]);
    let occupied = runtime.semaphore().try_acquire_many_owned(8).unwrap();
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(10),
        runtime.acquire(id, "pending")
    )
    .await
    .is_err());
    assert_eq!(runtime.snapshot().jobs, 0);
    drop(occupied);
    let id = start(&runtime, &["sent"]);
    let lease = runtime.acquire(id, "sent").await.unwrap();
    runtime.remember(id, "sent", "op".into());
    runtime.invalidate(ProbeJobState::InvalidatedByCoreRestart, 10);
    assert_eq!(runtime.expected(id, "sent"), None);
    assert_eq!(
        runtime
            .jobs
            .lock()
            .unwrap()
            .get_probe_job(id)
            .unwrap()
            .state,
        ProbeJobState::InvalidatedByCoreRestart
    );
    drop(lease);
    assert_eq!(runtime.snapshot().jobs, 0);
}

fn host() -> AppState {
    let profile = crate::models::proxy_config::ProxyConfigProfile {
        id: "test".into(),
        name: "test".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(serde_json::json!({})),
        active: true,
        updated_at_unix_ms: 1,
        capabilities: Default::default(),
    };
    AppState::with_domain_data(Default::default(), vec![profile], vec![], vec![], vec![])
}
#[tokio::test]
async fn host_lifecycle_invalidates_owned_jobs_and_does_not_reuse_old_scope() {
    let state = host();
    state.client_core_instance_started();
    let first = state
        .start_client_probe(request(vec!["a".into(), "b".into()]))
        .unwrap()
        .job;
    let lease = state.probe_runtime().acquire(first.id, "a").await.unwrap();
    state.client_core_configuration_committed(None);
    assert_eq!(
        state.get_client_probe_job(first.id).unwrap().state,
        ProbeJobState::InvalidatedByConfigChange
    );
    assert!(state.client_core_snapshot().active_probe_jobs.is_empty());
    assert!(state.probe_runtime().acquire(first.id, "b").await.is_none());
    assert_eq!(state.probe_runtime().snapshot().draining, 1);
    let late = crate::client_core::ProbeTargetResult {
        target_tag: "a".into(),
        reachable: true,
        latency_ms: Some(1),
        message: None,
        source: crate::client_core::ProbeObservationSource::ManualOutbound,
        observed_at_unix_ms: crate::services::common::now_unix_ms(),
    };
    assert!(state
        .record_client_probe_result(first.id, &first.scope, late)
        .is_none());
    drop(lease);
    assert_eq!(state.probe_runtime().snapshot().jobs, 0);
}
#[tokio::test]
async fn host_cancel_advances_snapshot_and_core_loss_preserves_terminal_state() {
    let state = host();
    let job = state
        .start_client_probe(request(vec!["a".into()]))
        .unwrap()
        .job;
    let lease = state.probe_runtime().acquire(job.id, "a").await.unwrap();
    state.probe_runtime().remember(job.id, "a", "op".into());
    let before = state.client_core_snapshot().revision;
    assert_eq!(
        state.cancel_client_probe(job.id).unwrap().state,
        ProbeJobState::Cancelled
    );
    assert!(state.client_core_snapshot().revision > before);
    assert_eq!(
        state.probe_runtime().expected(job.id, "a").as_deref(),
        Some("op")
    );
    state.client_core_instance_lost();
    assert_eq!(
        state.get_client_probe_job(job.id).unwrap().state,
        ProbeJobState::Cancelled
    );
    assert_eq!(state.probe_runtime().expected(job.id, "a"), None);
    drop(lease);
    let second = state
        .start_client_probe(request(vec!["b".into()]))
        .unwrap()
        .job;
    state.client_core_instance_started();
    assert_eq!(
        state.get_client_probe_job(second.id).unwrap().state,
        ProbeJobState::InvalidatedByCoreRestart
    );
    assert_eq!(state.probe_runtime().snapshot().jobs, 0);
}

#[test]
fn delayed_acknowledgement_cannot_resurrect_completed_or_disconnected_policy_work() {
    let runtime = ProbeRuntime::default();
    let id = start(&runtime, &["auto"]);
    runtime.remember(id, "auto", "request-id".into());
    runtime.acknowledge(id, "auto", "kernel-id".into());
    assert_eq!(runtime.expected(id, "auto").as_deref(), Some("kernel-id"));
    runtime.forget(id, "auto");
    runtime.acknowledge(id, "auto", "late-id".into());
    assert_eq!(runtime.expected(id, "auto"), None);
    runtime.remember(id, "auto", "request-id".into());
    runtime.invalidate(ProbeJobState::InvalidatedByCoreRestart, 10);
    runtime.acknowledge(id, "auto", "late-id".into());
    assert_eq!(runtime.expected(id, "auto"), None);
}
