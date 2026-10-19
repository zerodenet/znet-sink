use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn task_count_limit_preserves_readable_store_and_still_allows_existing_intent_updates() {
    let (root, host, _manager) = scheduled_fixture();
    let template = host
        .schedule_put(
            "org.example.plugin",
            "identity",
            "seed".into(),
            "refresh".into(),
            300,
        )
        .unwrap();
    let mut registry = Registry {
        schema_version: schema(),
        ..Default::default()
    };
    for index in 0..MAX_TASKS {
        let mut task = template.clone();
        task.task_id = format!("task-{index}");
        registry.tasks.insert(
            key(&task.plugin_id, &task.component_id, &task.task_id),
            task,
        );
    }
    save(root.path(), &registry).unwrap();
    let before = fs::read(path(root.path())).unwrap();
    assert!(host
        .schedule_put(
            "org.example.plugin",
            "identity",
            "overflow".into(),
            "refresh".into(),
            300
        )
        .is_err());
    assert_eq!(fs::read(path(root.path())).unwrap(), before);
    assert_eq!(load(root.path()).unwrap().tasks.len(), MAX_TASKS);
    host.schedule_put(
        "org.example.plugin",
        "identity",
        "task-0".into(),
        "refresh".into(),
        600,
    )
    .unwrap();
    assert_eq!(load(root.path()).unwrap().tasks.len(), MAX_TASKS);
    assert!(host
        .schedule_delete_checked("org.example.plugin", "identity", "../invalid", &|| Ok(()))
        .is_err());
}

fn scheduled_fixture() -> (tempfile::TempDir, Host, Manager) {
    let fixture = super::super::tests::setup_component(
        "42",
        serde_json::json!({"capability":"tasks.schedule","scope":"self"}),
    );
    let (_, host, manager) = &fixture;
    let _ = real_invocation(host, manager);
    fixture
}
fn queued(host: &Host, manager: &Manager) -> DueTask {
    host.schedule_put(
        "org.example.plugin",
        "identity",
        "refresh".into(),
        "refresh".into(),
        300,
    )
    .unwrap();
    let mut due = host.take_due(manager, u64::MAX).unwrap();
    assert_eq!(due.len(), 1);
    due.remove(0)
}

fn real_invocation(
    host: &Host,
    manager: &Manager,
) -> (Authority, znet_plugin_sandbox::policy::Lease, Request) {
    let request = Request {
        capability: Capability::TasksSchedule,
        scope: "self".into(),
    };
    let row = host.snapshot(manager).components.remove(0);
    host.authorize(manager, row.review.unwrap(), vec![request.clone()])
        .unwrap();
    let (component, authority) = host
        .sdk_context(manager, "org.example.plugin", "identity", &request)
        .unwrap();
    let lease = authority
        .begin_host_call(&component, Duration::from_secs(10), 64 * 1024)
        .unwrap();
    (authority, lease, request)
}

#[test]
fn same_value_schedule_replacement_rejects_the_original_completion() {
    let (root, host, _manager) = scheduled_fixture();
    let old = host
        .schedule_put(
            "org.example.plugin",
            "identity",
            "refresh".into(),
            "refresh".into(),
            300,
        )
        .unwrap();
    let generation = host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .installation_generation;
    let new = host
        .schedule_put(
            "org.example.plugin",
            "identity",
            "refresh".into(),
            "refresh".into(),
            300,
        )
        .unwrap();
    assert_eq!(new.revision.len(), 48);
    assert_ne!(old.revision, new.revision);
    let before = fs::read(path(root.path())).unwrap();
    assert!(host
        .record_schedule_result(&old, generation, false, 1000)
        .is_err());
    assert_eq!(fs::read(path(root.path())).unwrap(), before);
    host.record_schedule_result(&new, generation, false, 1000)
        .unwrap();
    assert_eq!(
        load(root.path())
            .unwrap()
            .tasks
            .values()
            .next()
            .unwrap()
            .failure_count,
        1
    );
}

#[test]
fn queued_task_never_borrows_a_replacement_intent_or_reenabled_grant() {
    let (root, host, manager) = scheduled_fixture();
    let old = queued(&host, &manager);
    {
        let _operation = host.operation_guard().unwrap();
        host.checked_scheduled_due(&manager, &old).unwrap();
    }
    manager.revoke_component(&old.review.key);
    host.checked_rescan(&manager).unwrap(); // Durable consent restored with a new real epoch.
    let before = fs::read(path(root.path())).unwrap();
    assert!(host
        .record_scheduled_completion(&manager, &old, true, 1000)
        .is_err());
    assert_eq!(fs::read(path(root.path())).unwrap(), before);
    {
        let _operation = host.operation_guard().unwrap();
        assert!(host.checked_scheduled_due(&manager, &old).is_err());
    }
    let fresh = host.take_due(&manager, u64::MAX).unwrap().remove(0);
    assert_ne!(fresh.review.revision, old.review.revision);
    {
        let _operation = host.operation_guard().unwrap();
        host.checked_scheduled_due(&manager, &fresh).unwrap();
    }
    host.schedule_put(
        "org.example.plugin",
        "identity",
        "refresh".into(),
        "refresh".into(),
        300,
    )
    .unwrap();
    let before = fs::read(path(root.path())).unwrap();
    {
        let _operation = host.operation_guard().unwrap();
        assert!(host.checked_scheduled_due(&manager, &fresh).is_err());
    }
    assert!(host
        .record_scheduled_completion(&manager, &fresh, false, 1000)
        .is_err());
    assert_eq!(fs::read(path(root.path())).unwrap(), before);
}

#[test]
fn queued_task_cannot_invoke_same_material_after_actual_store_reinstallation() {
    let (root, host, manager) = scheduled_fixture();
    let mut due = queued(&host, &manager);
    let registration = host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find("org.example.plugin")
        .unwrap()
        .clone();
    let store = host.store().unwrap();
    let (review, _) = store.review_uninstall(&registration).unwrap();
    let bytes = fs::read(
        root.path()
            .join("packages")
            .join(format!("{}.zspkg", review.current_sha256)),
    )
    .unwrap();
    store.uninstall(&registration.id).unwrap();
    store
        .install(
            &bytes,
            &registration,
            &Target::native_desktop().unwrap(),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
    host.checked_rescan(&manager).unwrap();
    // Even accidentally refreshing only the review cannot make an old installation current.
    due.review = host.snapshot(&manager).components.remove(0).review.unwrap();
    let before = fs::read(path(root.path())).unwrap();
    {
        let _operation = host.operation_guard().unwrap();
        assert!(host.checked_scheduled_due(&manager, &due).is_err());
    }
    assert!(host
        .record_scheduled_completion(&manager, &due, true, 1000)
        .is_err());
    assert_eq!(fs::read(path(root.path())).unwrap(), before);
}

#[test]
fn revoked_queued_schedule_put_and_delete_cannot_change_the_store() {
    for deleting in [false, true] {
        let (root, host, manager) = scheduled_fixture();
        host.schedule_put(
            "org.example.plugin",
            "identity",
            "refresh".into(),
            "refresh".into(),
            300,
        )
        .unwrap();
        let (authority, lease, request) = real_invocation(&host, &manager);
        let before = fs::read(path(root.path())).unwrap();
        let host = Arc::new(host);
        let held = host.operation.lock().unwrap();
        let (started, waiting) = std::sync::mpsc::sync_channel(1);
        let worker = {
            let host = host.clone();
            std::thread::spawn(move || {
                started.send(()).unwrap();
                let check = || {
                    lease
                        .check(Some(&request))
                        .map_err(super::super::sdk::invocation_failure)
                };
                if deleting {
                    host.schedule_delete_checked(
                        "org.example.plugin",
                        "identity",
                        "refresh",
                        &check,
                    )
                } else {
                    host.schedule_put_checked(
                        "org.example.plugin",
                        "identity",
                        "refresh".into(),
                        "replace".into(),
                        600,
                        &check,
                    )
                    .map(|_| ())
                }
            })
        };
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        authority.revoke();
        drop(held);
        assert!(worker.join().unwrap().is_err());
        assert_eq!(fs::read(path(root.path())).unwrap(), before);
    }
}

#[test]
fn schedule_mutations_check_original_invocation_again_at_publication() {
    for deleting in [false, true] {
        let (root, host, manager) = scheduled_fixture();
        host.schedule_put(
            "org.example.plugin",
            "identity",
            "refresh".into(),
            "refresh".into(),
            300,
        )
        .unwrap();
        let (authority, lease, request) = real_invocation(&host, &manager);
        let before = fs::read(path(root.path())).unwrap();
        let calls = AtomicUsize::new(0);
        let check = || {
            if calls.fetch_add(1, Ordering::SeqCst) == 1 {
                authority.revoke();
            }
            lease
                .check(Some(&request))
                .map_err(super::super::sdk::invocation_failure)
        };
        let result = if deleting {
            host.schedule_delete_checked("org.example.plugin", "identity", "refresh", &check)
        } else {
            host.schedule_put_checked(
                "org.example.plugin",
                "identity",
                "refresh".into(),
                "replace".into(),
                600,
                &check,
            )
            .map(|_| ())
        };
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(fs::read(path(root.path())).unwrap(), before);
    }
}

#[test]
fn registry_round_trips_only_bounded_tasks() {
    let root = tempfile::tempdir().unwrap();
    let task = Task {
        plugin_id: "org.example.plugin".into(),
        component_id: "worker".into(),
        task_id: "refresh".into(),
        action: "refresh".into(),
        interval_seconds: MIN_INTERVAL_SECONDS,
        next_run_unix_ms: 100,
        failure_count: 0,
        revision: String::new(),
    };
    let mut registry = Registry {
        schema_version: schema(),
        ..Registry::default()
    };
    registry.tasks.insert(
        key(&task.plugin_id, &task.component_id, &task.task_id),
        task,
    );
    save(root.path(), &registry).unwrap();
    assert_eq!(load(root.path()).unwrap().tasks.len(), 1);
    assert!(!safe_id("../escape"));
}

#[test]
fn failed_tasks_use_bounded_exponential_retry_and_success_resets_it() {
    let (root, host, _manager) = scheduled_fixture();
    let installation_generation = host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .installation_generation;
    let task = Task {
        plugin_id: "org.example.plugin".into(),
        component_id: "identity".into(),
        task_id: "refresh".into(),
        action: "refresh".into(),
        interval_seconds: 900,
        next_run_unix_ms: 100,
        failure_count: 0,
        revision: String::new(),
    };
    let mut registry = Registry {
        schema_version: schema(),
        ..Registry::default()
    };
    registry.tasks.insert(
        key(&task.plugin_id, &task.component_id, &task.task_id),
        task.clone(),
    );
    save(root.path(), &registry).unwrap();
    host.record_schedule_result(&task, installation_generation, false, 1_000)
        .unwrap();
    let failed = load(root.path())
        .unwrap()
        .tasks
        .into_values()
        .next()
        .unwrap();
    assert_eq!(failed.failure_count, 1);
    assert_eq!(failed.next_run_unix_ms, 61_000);
    host.record_schedule_result(&failed, installation_generation, true, 2_000)
        .unwrap();
    let recovered = load(root.path())
        .unwrap()
        .tasks
        .into_values()
        .next()
        .unwrap();
    assert_eq!(recovered.failure_count, 0);
    assert_eq!(recovered.next_run_unix_ms, 902_000);
}
