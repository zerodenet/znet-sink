use super::*;
use crate::{
    contract::*,
    policy::Authority,
    runtime::{HostSdkDispatcher, ReviewedExecution},
};
use std::collections::BTreeSet;
use znet_client_core::capability::Manager;

struct Pool(Arc<Broker>);
impl Pool {
    fn new(workers: usize, queue: usize) -> Self {
        Self(Broker::new(workers, queue))
    }
    fn owner(&self) -> Executor {
        self.0.owner()
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        let mut s = self.0.state.lock().unwrap();
        assert!(s.active.is_empty());
        assert!(s.queue.is_empty());
        s.failed = true;
        self.0.changed.notify_all();
    }
}
fn component(id: &str, source: &str) -> Component {
    let m = Manifest {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.worker".into(),
        component_id: id.into(),
        version: "1.0.0".into(),
        requires_host: "=0.0.1".into(),
        api_version: 1,
        runtime: "javascript-v1".into(),
        minimum_isolation: Isolation::Vm,
        targets: Targets::Only(vec![Target::native_desktop().unwrap()]),
        required: vec![],
        optional: vec![],
        requires_methods: vec![],
        configuration: None,
        lifecycle: vec![],
        source_sha256: sha256(source.as_bytes()),
        limits: Limits {
            timeout_ms: 2000,
            ..Limits::default()
        },
    };
    Component::load(&serde_json::to_vec(&m).unwrap(), source).unwrap()
}
fn admitted(id: &str) -> (Authority, Arc<Lease>, Arc<AtomicBool>) {
    let c = component(id, "1");
    let a = Authority::admit(&c, &BTreeSet::new()).unwrap();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let lease = Arc::new(a.begin_scheduled(&c, cancel.clone()).unwrap());
    (a, lease, cancel)
}
fn wait_for(check: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !check() {
        assert!(Instant::now() < deadline, "worker condition timed out");
        thread::yield_now();
    }
}
fn result_error(result: Result<ExecutionReceipt, Error>) -> Error {
    match result {
        Err(e) => e,
        Ok(_) => panic!("expected denied result"),
    }
}

#[test]
fn reviewed_target_must_match_native_abi_even_for_an_any_device_manifest() {
    let source = "42";
    let original = component("native-target", source);
    let mut manifest = original.manifest().clone();
    manifest.targets = Targets::Any(AnyTarget::Any);
    let component = Component::load(&serde_json::to_vec(&manifest).unwrap(), source).unwrap();
    let manager = Manager::default();
    let authority = Authority::admit_with_manager(&component, &BTreeSet::new(), &manager).unwrap();
    authority
        .authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let binding = manager.component_snapshots().into_iter().next().unwrap();
    let target = Target::native_desktop().unwrap();
    let mut forged = target.clone();
    forged.arch = if target.arch == Arch::Aarch64 {
        Arch::X86_64
    } else {
        Arch::Aarch64
    };
    let reviewed = |target| ReviewedExecution {
        target,
        registration: binding.registration,
        revision: binding.revision,
        cancelled: Arc::new(AtomicBool::new(false)),
        host_version: "0.0.1".into(),
        dispatcher: None,
        scheduled: false,
    };
    let executor = Executor::default();
    assert_eq!(
        result_error(executor.execute_reviewed(
            &component,
            &authority,
            Value::Null,
            reviewed(forged)
        )),
        Error::IncompatibleDevice
    );
    assert!(!manager.component_snapshots()[0].running);
    assert_eq!(
        executor
            .execute_reviewed(&component, &authority, Value::Null, reviewed(target))
            .unwrap()
            .into_value()
            .unwrap(),
        42
    );
}

#[test]
fn shared_budget_rejects_overflow_and_shutdown_reports_real_sdk_draining() {
    let pool = Pool::new(1, 1);
    let first = pool.owner();
    let other = pool.owner();
    let (_a, l, c) = admitted("first");
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let owner = first.clone();
    let running = thread::spawn(move || {
        owner.execute(
            l,
            c,
            Box::new(move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Value::from(1))
            }),
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let (_a, l, c) = admitted("queued");
    let queued_started = Arc::new(AtomicBool::new(false));
    let flag = queued_started.clone();
    let owner = first.clone();
    let queued = thread::spawn(move || {
        owner.execute(
            l,
            c,
            Box::new(move || {
                flag.store(true, Ordering::Release);
                Ok(Value::from(2))
            }),
        )
    });
    wait_for(|| first.snapshot().global_queued == 1);
    let (_a, l, c) = admitted("overflow");
    assert_eq!(
        result_error(other.execute(l, c, Box::new(|| Ok(Value::Null)))),
        Error::Busy
    );
    let report = first.close(Duration::from_millis(10));
    assert!(!report.accepting);
    assert_eq!(report.queued, 0);
    assert_eq!(report.running, 1);
    assert_eq!(report.draining, 1);
    assert!(!report.drained);
    assert_eq!(result_error(queued.join().unwrap()), Error::Cancelled);
    assert!(!queued_started.load(Ordering::Acquire));
    let (_a, l, c) = admitted("closed");
    assert_eq!(
        result_error(first.execute(l, c, Box::new(|| Ok(Value::Null)))),
        Error::Cancelled
    );
    release_tx.send(()).unwrap();
    assert_eq!(result_error(running.join().unwrap()), Error::Cancelled);
    assert!(first.close(Duration::from_secs(1)).drained);
    let (_a, l, c) = admitted("other");
    assert_eq!(
        other
            .execute(l, c, Box::new(|| Ok(Value::from(3))))
            .unwrap()
            .into_value()
            .unwrap(),
        Value::from(3)
    );
}
#[test]
fn queued_revocation_never_dispatches_and_late_active_result_is_denied() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let (active_authority, l, c) = admitted("active");
    let (tx, rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let owner = executor.clone();
    let running = thread::spawn(move || {
        owner.execute(
            l,
            c,
            Box::new(move || {
                tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Value::from("late"))
            }),
        )
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let (queued_authority, l, c) = admitted("queued");
    let invoked = Arc::new(AtomicBool::new(false));
    let flag = invoked.clone();
    let owner = executor.clone();
    let queued = thread::spawn(move || {
        owner.execute(
            l,
            c,
            Box::new(move || {
                flag.store(true, Ordering::Release);
                Ok(Value::Null)
            }),
        )
    });
    wait_for(|| executor.snapshot().queued == 1);
    queued_authority.revoke();
    // Queue cancellation completes while the unrelated native callback remains
    // blocked; it does not wait for a free VM thread to observe revocation.
    assert_eq!(result_error(queued.join().unwrap()), Error::Revoked);
    assert_eq!(executor.snapshot().running, 1);
    active_authority.revoke();
    release_tx.send(()).unwrap();
    assert_eq!(result_error(running.join().unwrap()), Error::Revoked);
    assert!(!invoked.load(Ordering::Acquire));
}
#[test]
fn receipt_keeps_original_busy_lease_and_denies_stop_restart_aba() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let (authority, l, cancel) = admitted("receipt");
    let receipt = executor
        .execute(l, cancel, Box::new(|| Ok(Value::from("result"))))
        .unwrap();
    let c = component("receipt", "1");
    assert!(matches!(authority.begin(&c), Err(Error::Busy)));
    authority.revoke();
    authority
        .authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    assert_eq!(receipt.check(), Err(Error::Revoked));
    drop(receipt);
    assert!(authority.begin(&c).is_ok());
}
#[test]
fn rust_adapter_panic_has_fixed_failure_and_worker_accepts_next_job() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let (_a, l, c) = admitted("panic");
    assert_eq!(
        result_error(executor.execute(l, c, Box::new(|| panic!("secret-panic-canary")))),
        Error::HostFailure
    );
    let (_a, l, c) = admitted("next");
    assert_eq!(
        executor
            .execute(l, c, Box::new(|| Ok(Value::from("ok"))))
            .unwrap()
            .into_value()
            .unwrap(),
        Value::from("ok")
    );
}
#[test]
fn reviewed_real_vm_runs_on_worker_and_rejects_registration_and_epoch_changes() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let manager = Manager::default();
    let c = component("vm", "JSON.parse(hostSdkCall('{}'))");
    let a = Authority::admit_with_manager(&c, &BTreeSet::new(), &manager).unwrap();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let review = manager.component_snapshots().remove(0);
    let dispatch: HostSdkDispatcher = Arc::new(|_, _| {
        assert!(thread::current()
            .name()
            .unwrap()
            .starts_with("znet-plugin-vm-"));
        Ok("\"real VM\"".into())
    });
    let options = || ReviewedExecution {
        target: Target::native_desktop().unwrap(),
        registration: review.registration,
        revision: review.revision,
        cancelled: Arc::new(AtomicBool::new(false)),
        host_version: "0.0.1".into(),
        dispatcher: Some(dispatch.clone()),
        scheduled: false,
    };
    assert_eq!(
        executor
            .execute_reviewed(&c, &a, Value::Null, options())
            .unwrap()
            .into_value()
            .unwrap(),
        Value::from("real VM")
    );
    let mut wrong = options();
    wrong.registration += 1;
    assert_eq!(
        result_error(executor.execute_reviewed(&c, &a, Value::Null, wrong)),
        Error::Revoked
    );
    a.revoke();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    assert_eq!(
        result_error(executor.execute_reviewed(&c, &a, Value::Null, options())),
        Error::Revoked
    );
}
#[test]
fn page_cancel_during_real_sdk_wait_prevents_second_call_and_result_delivery() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let manager = Manager::default();
    let c = component("cancel", "hostSdkCall('{}'); hostSdkCall('{}'); 'late'");
    let a = Authority::admit_with_manager(&c, &BTreeSet::new(), &manager).unwrap();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let review = manager.component_snapshots().remove(0);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let dispatch: HostSdkDispatcher = Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::Relaxed);
        flag.store(true, Ordering::Release);
        Ok("null".into())
    });
    let result = executor.execute_reviewed(
        &c,
        &a,
        Value::Null,
        ReviewedExecution {
            target: Target::native_desktop().unwrap(),
            registration: review.registration,
            revision: review.revision,
            cancelled: cancel,
            host_version: "0.0.1".into(),
            dispatcher: Some(dispatch),
            scheduled: false,
        },
    );
    assert_eq!(result_error(result), Error::Cancelled);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn two_worker_slots_are_shared_across_owners_and_queue_wait_preserves_guest_cpu_budget() {
    let pool = Pool::new(GLOBAL_WORKERS, 2);
    let first = pool.owner();
    let other = pool.owner();
    let (entered_tx, entered_rx) = mpsc::sync_channel(GLOBAL_WORKERS);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let mut runs = vec![];
    for index in 0..GLOBAL_WORKERS {
        let owner = if index == 0 {
            first.clone()
        } else {
            other.clone()
        };
        let (_a, l, c) = admitted(&format!("occupied{index}"));
        let tx = entered_tx.clone();
        let gate = gate.clone();
        runs.push(thread::spawn(move || {
            owner.execute(
                l,
                c,
                Box::new(move || {
                    tx.send(()).unwrap();
                    let (lock, changed) = &*gate;
                    let mut done = lock.lock().unwrap();
                    while !*done {
                        done = changed.wait(done).unwrap();
                    }
                    Ok(Value::Null)
                }),
            )
        }));
    }
    for _ in 0..GLOBAL_WORKERS {
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    assert_eq!(first.snapshot().global_running, GLOBAL_WORKERS);
    let manager = Manager::default();
    let mut c = component("short", "'bounded'");
    c.manifest.limits.timeout_ms = 10; // signed fixture only; real component limits remain immutable outside crate
    let a = Authority::admit_with_manager(&c, &BTreeSet::new(), &manager).unwrap();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let review = manager.component_snapshots().remove(0);
    let owner = other.clone();
    let queued = thread::spawn(move || {
        owner.execute_reviewed(
            &c,
            &a,
            Value::Null,
            ReviewedExecution {
                target: Target::native_desktop().unwrap(),
                registration: review.registration,
                revision: review.revision,
                cancelled: Arc::new(AtomicBool::new(false)),
                host_version: "0.0.1".into(),
                dispatcher: None,
                scheduled: false,
            },
        )
    });
    wait_for(|| first.snapshot().global_queued == 1);
    thread::sleep(Duration::from_millis(30));
    assert_eq!(first.snapshot().global_running, GLOBAL_WORKERS);
    let (lock, changed) = &*gate;
    *lock.lock().unwrap() = true;
    changed.notify_all();
    for run in runs {
        run.join().unwrap().unwrap().into_value().unwrap();
    }
    assert_eq!(
        queued.join().unwrap().unwrap().into_value().unwrap(),
        Value::from("bounded")
    );
}
#[test]
fn real_vm_sdk_panic_is_fixed_failure_without_replacing_guest_or_host_permissions() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let manager = Manager::default();
    let c = component(
        "sdkpanic",
        "try { hostSdkCall('{}'); } catch (_) {} 'hidden'",
    );
    let a = Authority::admit_with_manager(&c, &BTreeSet::new(), &manager).unwrap();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let review = manager.component_snapshots().remove(0);
    let result = executor.execute_reviewed(
        &c,
        &a,
        Value::Null,
        ReviewedExecution {
            target: Target::native_desktop().unwrap(),
            registration: review.registration,
            revision: review.revision,
            cancelled: Arc::new(AtomicBool::new(false)),
            host_version: "0.0.1".into(),
            dispatcher: Some(Arc::new(|_, _| panic!("native-secret-canary"))),
            scheduled: false,
        },
    );
    assert_eq!(result_error(result), Error::HostFailure);
    assert!(manager.component_snapshots()[0].enabled);
}

#[test]
fn ordinary_slow_sdk_keeps_original_wall_deadline_while_scheduled_keeps_separate_wall_limit() {
    let pool = Pool::new(1, 2);
    let executor = pool.owner();
    let manager = Manager::default();
    let mut c = component("wall", "hostSdkCall('{}'); 'late'");
    c.manifest.limits.timeout_ms = 20;
    let a = Authority::admit_with_manager(&c, &BTreeSet::new(), &manager).unwrap();
    a.authorize(BTreeSet::new(), Duration::from_secs(60))
        .unwrap();
    let review = manager.component_snapshots().remove(0);
    let options = |scheduled| ReviewedExecution {
        target: Target::native_desktop().unwrap(),
        registration: review.registration,
        revision: review.revision,
        cancelled: Arc::new(AtomicBool::new(false)),
        host_version: "0.0.1".into(),
        dispatcher: Some(Arc::new(|_, _| {
            thread::sleep(Duration::from_millis(40));
            Ok("null".into())
        })),
        scheduled,
    };
    assert_eq!(
        result_error(executor.execute_reviewed(&c, &a, Value::Null, options(false))),
        Error::Deadline
    );
    assert_eq!(
        executor
            .execute_reviewed(&c, &a, Value::Null, options(true))
            .unwrap()
            .into_value()
            .unwrap(),
        Value::from("late")
    );
    assert_eq!(executor.snapshot().ordinary_execution_wall_max_ms, 2000);
    assert_eq!(executor.snapshot().queue_wait_limit_ms, 5000);
    assert_eq!(executor.snapshot().scheduled_execution_wall_max_ms, 120000);
}
