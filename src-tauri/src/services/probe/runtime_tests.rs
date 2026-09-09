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
