use super::tests::{queued, scheduled_fixture};
use super::*;

fn read_task(host: &Host) -> Task {
    host.schedule_list("org.example.plugin", "identity")
        .unwrap()
        .remove(0)
}
fn mark_dispatch(host: &Host, manager: &Manager, due: &DueTask) {
    let _operation = host.operation_guard().unwrap();
    host.mark_scheduled_dispatch(manager, due, 1234).unwrap();
}

#[test]
fn queued_original_is_not_dispatched_twice_and_owned_clones_keep_it_live() {
    let (_root, host, manager) = scheduled_fixture();
    let original = queued(&host, &manager);
    let id = original.task.last_attempt.as_ref().unwrap().id.clone();
    assert_eq!(id.len(), 48);
    assert_eq!(read_task(&host).last_attempt.unwrap().state, Phase::Queued);
    let held = original.clone();
    drop(original);
    assert!(host.take_due(&manager, u64::MAX).unwrap().is_empty());
    assert_eq!(read_task(&host).last_attempt.unwrap().id, id);
    drop(held);
    // An abandoned queued attempt never crossed the durable dispatch marker.
    let next_time = read_task(&host).next_run_unix_ms;
    assert!(host
        .take_due(&manager, next_time.saturating_sub(1))
        .unwrap()
        .is_empty());
    let cancelled = read_task(&host).last_attempt.unwrap();
    assert_eq!(cancelled.state, Phase::Cancelled);
    assert!(cancelled.dispatch_at_unix_ms.is_none());
    assert!(cancelled.completed_at_unix_ms.is_none());
    let fresh = host.take_due(&manager, next_time).unwrap().remove(0);
    assert_ne!(fresh.task.last_attempt.as_ref().unwrap().id, id);
}

#[test]
fn lost_running_owner_is_unconfirmed_and_never_automatically_replayed() {
    let (root, host, manager) = scheduled_fixture();
    let original = queued(&host, &manager);
    mark_dispatch(&host, &manager, &original);
    let before = fs::read(path(root.path())).unwrap();
    {
        let _operation = host.operation_guard().unwrap();
        assert!(host
            .mark_scheduled_dispatch(&manager, &original, 5678)
            .is_err());
    }
    assert_eq!(fs::read(path(root.path())).unwrap(), before);
    drop(original);
    assert!(host.take_due(&manager, u64::MAX).unwrap().is_empty());
    let old = read_task(&host);
    let attempt = old.last_attempt.as_ref().unwrap();
    assert_eq!(attempt.state, Phase::Unconfirmed);
    assert!(attempt.completed_at_unix_ms.is_none());
    let reconciled = fs::read(path(root.path())).unwrap();
    assert!(host.take_due(&manager, u64::MAX).unwrap().is_empty());
    assert_eq!(fs::read(path(root.path())).unwrap(), reconciled);
    let replaced = host
        .schedule_put(
            "org.example.plugin",
            "identity",
            "refresh".into(),
            "refresh".into(),
            300,
        )
        .unwrap();
    assert_ne!(replaced.revision, old.revision);
    assert!(replaced.last_attempt.is_none());
    assert_eq!(host.take_due(&manager, u64::MAX).unwrap().len(), 1);
}

#[test]
fn process_owner_reconstruction_cannot_inherit_old_inflight_authority() {
    let (_root, mut host, manager) = scheduled_fixture();
    let original = queued(&host, &manager);
    mark_dispatch(&host, &manager, &original);
    // Reconstruct only the process-local owner. No persisted lease is restored.
    host.scheduler = Default::default();
    assert!(host.take_due(&manager, u64::MAX).unwrap().is_empty());
    assert_eq!(
        read_task(&host).last_attempt.unwrap().state,
        Phase::Unconfirmed
    );
    assert!(host
        .record_scheduled_completion(&manager, &original, Completion::Known(true), 9999)
        .is_err());
}

#[test]
fn original_authority_replacement_cannot_turn_running_work_into_a_new_attempt() {
    let (_root, host, manager) = scheduled_fixture();
    let original = queued(&host, &manager);
    mark_dispatch(&host, &manager, &original);
    manager.revoke_component(&original.review.key);
    host.checked_rescan(&manager).unwrap();
    assert!(host.take_due(&manager, u64::MAX).unwrap().is_empty());
    assert_eq!(
        read_task(&host).last_attempt.unwrap().state,
        Phase::Unconfirmed
    );
    assert!(host
        .record_scheduled_completion(&manager, &original, Completion::Known(true), 9999)
        .is_err());
}

#[test]
fn accepted_completion_and_declared_retry_have_explicit_facts_but_unknown_does_not() {
    for completion in [
        Completion::Known(true),
        Completion::Known(false),
        Completion::Unconfirmed,
    ] {
        let (_root, host, manager) = scheduled_fixture();
        let original = queued(&host, &manager);
        mark_dispatch(&host, &manager, &original);
        host.record_scheduled_completion(&manager, &original, completion, 2000)
            .unwrap();
        let task = read_task(&host);
        let attempt = task.last_attempt.unwrap();
        match completion {
            Completion::Known(success) => {
                assert_eq!(
                    attempt.state,
                    if success {
                        Phase::Completed
                    } else {
                        Phase::Failed
                    }
                );
                assert_eq!(attempt.completed_at_unix_ms.as_deref(), Some("2000"));
                assert_eq!(task.failure_count, u8::from(!success));
                assert_eq!(host.take_due(&manager, u64::MAX).unwrap().len(), 1);
            }
            Completion::Unconfirmed => {
                assert_eq!(attempt.state, Phase::Unconfirmed);
                assert!(attempt.completed_at_unix_ms.is_none());
                assert_eq!(task.failure_count, 0);
                assert!(host.take_due(&manager, u64::MAX).unwrap().is_empty());
            }
        }
        assert_eq!(attempt.dispatch_at_unix_ms.as_deref(), Some("1234"));
    }
}

#[test]
fn journal_round_trip_keeps_lossless_timestamps_and_rejects_impossible_phases() {
    let (root, host, manager) = scheduled_fixture();
    let original = queued(&host, &manager);
    let task = read_task(&host);
    let json = serde_json::to_value(&task).unwrap();
    assert_eq!(json["lastAttempt"]["queuedAtUnixMs"], u64::MAX.to_string());
    let public: znet_plugin_sandbox::sdk::ScheduledTask =
        serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        public.last_attempt.unwrap().queued_at_unix_ms,
        u64::MAX.to_string()
    );
    assert_eq!(public.revision.as_deref(), Some(task.revision.as_str()));
    assert_eq!(
        json["lastAttempt"]["completedAtUnixMs"],
        serde_json::Value::Null
    );
    let mut registry = load(root.path()).unwrap();
    registry
        .tasks
        .values_mut()
        .next()
        .unwrap()
        .last_attempt
        .as_mut()
        .unwrap()
        .dispatch_at_unix_ms = Some("1234".into());
    // Simulate malformed local recovery data; it cannot become a runnable task.
    fs::write(path(root.path()), serde_json::to_vec(&registry).unwrap()).unwrap();
    assert!(load(root.path()).is_err());
    assert!(host.take_due(&manager, u64::MAX).is_err());
    drop(original);
}

#[test]
fn repeated_resume_has_only_one_native_worker_owner() {
    let owner = Arc::new(Owner::default());
    let attempts: Vec<_> = (0..16)
        .map(|_| {
            let owner = owner.clone();
            std::thread::spawn(move || owner.start())
        })
        .collect();
    assert_eq!(
        attempts
            .into_iter()
            .map(|a| a.join().unwrap())
            .filter(|started| *started)
            .count(),
        1
    );
    assert!(!owner.start());
    owner.close();
    assert!(owner.is_closed());
    assert!(!owner.start());
}
