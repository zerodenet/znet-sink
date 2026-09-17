use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use znet_client_core::capability::*;
fn permission() -> Permission {
    Permission::new("network.get", "https://example.org")
}
fn policy(manager: &Manager, name: &str) -> Policy {
    let grants = BTreeSet::from([permission()]);
    let policy = manager
        .admit(name.into(), grants.clone(), grants.clone(), &grants)
        .unwrap();
    policy.authorize(grants, Duration::from_secs(60)).unwrap();
    policy
}
fn budget(calls: u32, bytes: usize) -> Budget {
    Budget {
        calls,
        resource_bytes: bytes,
        timeout: Duration::from_secs(1),
    }
}
fn lease(policy: &Policy, calls: u32, bytes: usize) -> Lease {
    policy
        .begin(budget(calls, bytes), Arc::new(AtomicBool::new(false)))
        .unwrap()
}
#[test]
fn scope_denial_precedes_executor_and_identity_is_host_owned() {
    let manager = Manager::default();
    let policy = policy(&manager, "component-digest");
    let run = lease(&policy, 1, 32);
    assert_eq!(run.identity(), "component-digest");
    assert!(matches!(
        run.execute(
            &Permission::new("network.get", "https://other.org"),
            || -> Result<((), usize), Error> { panic!("must not execute") }
        ),
        Err(Error::PermissionDenied)
    ));
    assert!(manager.operations().is_empty());
}
#[test]
fn revoked_late_result_is_not_a_success_or_rollback() {
    let manager = Manager::default();
    let policy = policy(&manager, "plugin");
    let run = lease(&policy, 1, 32);
    assert!(matches!(
        run.execute(&permission(), || {
            policy.revoke();
            Ok(("secret", 6))
        }),
        Err(Error::Revoked)
    ));
    assert_eq!(
        manager.operations()[0].state,
        OperationState::DeliveryDenied(Error::Revoked)
    );
}
#[test]
fn resources_cannot_cross_invocations_or_survive_reauthorization() {
    let manager = Manager::default();
    let a = policy(&manager, "a");
    let b = policy(&manager, "b");
    let first = lease(&a, 2, 32);
    let other = lease(&b, 1, 32);
    let resource = first.execute(&permission(), || Ok(("secret", 6))).unwrap();
    assert_eq!(resource.take(&other), Err(Error::PermissionDenied));
    let resource = first.execute(&permission(), || Ok(("secret", 6))).unwrap();
    a.authorize(BTreeSet::from([permission()]), Duration::from_secs(60))
        .unwrap();
    assert_eq!(resource.take(&first), Err(Error::Revoked));
    assert!(matches!(
        a.begin(budget(1, 32), Arc::new(AtomicBool::new(false))),
        Err(Error::Busy)
    ));
    drop(first);
    assert!(a
        .begin(budget(1, 32), Arc::new(AtomicBool::new(false)))
        .is_ok());
}
#[test]
fn resources_release_capacity_but_do_not_refund_call_budget() {
    let manager = Manager::default();
    let policy = policy(&manager, "plugin");
    let run = lease(&policy, 3, 4);
    let resource = run.execute(&permission(), || Ok((42, 4))).unwrap();
    assert!(matches!(
        run.execute(&permission(), || Ok((99, 1))),
        Err(Error::BudgetExceeded)
    ));
    drop(resource);
    assert_eq!(
        run.execute(&permission(), || Ok((7, 4)))
            .unwrap()
            .take(&run),
        Ok(7)
    );
    assert!(matches!(
        run.execute(&permission(), || -> Result<((), usize), Error> {
            panic!("call budget")
        }),
        Err(Error::BudgetExceeded)
    ));
}
#[test]
fn cancelled_before_and_during_execution_never_delivers() {
    let manager = Manager::default();
    let policy = policy(&manager, "plugin");
    let cancelled = Arc::new(AtomicBool::new(false));
    let run = policy.begin(budget(2, 32), cancelled.clone()).unwrap();
    assert!(matches!(
        run.execute(&permission(), || {
            cancelled.store(true, Ordering::Relaxed);
            Ok((1, 1))
        }),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        run.execute(&permission(), || -> Result<((), usize), Error> {
            panic!("cancelled")
        }),
        Err(Error::Cancelled)
    ));
}

#[test]
fn shutdown_cancels_native_leases_and_closes_future_admission() {
    let manager = Manager::default();
    let policy = policy(&manager, "native");
    let run = lease(&policy, 1, 32);
    manager.shutdown();
    assert_eq!(run.check(None), Err(Error::Cancelled));
    assert!(matches!(
        manager.admit(
            "late".into(),
            BTreeSet::from([permission()]),
            BTreeSet::from([permission()]),
            &BTreeSet::from([permission()])
        ),
        Err(Error::Cancelled)
    ));
}
#[test]
fn operation_history_is_bounded_without_payloads() {
    let manager = Manager::default();
    let policy = policy(&manager, "plugin");
    for _ in 0..2 {
        let run = lease(&policy, 200, 32);
        for _ in 0..200 {
            drop(
                run.execute(&permission(), || Ok(("canary-secret", 13)))
                    .unwrap(),
            );
        }
    }
    let operations = manager.operations();
    assert_eq!(operations.len(), 256);
    assert!(operations
        .iter()
        .all(|o| o.state == OperationState::Completed));
    assert!(!format!("{operations:?}").contains("canary-secret"));
    assert!(operations.windows(2).all(|w| w[0].id < w[1].id));
}

#[test]
fn concurrent_execution_limit_does_not_serialize_unrelated_operations() {
    fn nested(manager: &Manager, depth: usize) {
        assert!(depth <= 32, "manager failed to enforce concurrency budget");
        let policy = policy(manager, &format!("caller-{depth}"));
        let run = lease(&policy, 1, 32);
        let result = run.execute(&permission(), || {
            nested(manager, depth + 1);
            Ok(((), 0))
        });
        if depth == 32 {
            assert!(matches!(result, Err(Error::Busy)));
        } else {
            assert!(result.is_ok());
        }
    }
    let manager = Manager::default();
    nested(&manager, 0);
    assert_eq!(manager.operations().len(), 32);
    assert!(manager
        .operations()
        .iter()
        .all(|o| o.state == OperationState::Completed));
}

#[test]
fn target_claim_prevents_cross_caller_overlap_and_releases_on_drop() {
    let manager = Manager::default();
    let a = lease(&policy(&manager, "a"), 1, 32);
    let b = lease(&policy(&manager, "b"), 1, 32);
    let claim = a.claim_resource("runtime:one".into()).unwrap();
    assert!(matches!(
        b.claim_resource("runtime:one".into()),
        Err(Error::Busy)
    ));
    let other = b.claim_resource("runtime:two".into()).unwrap();
    drop(claim);
    assert!(b.claim_resource("runtime:one".into()).is_ok());
    drop(other);
}

#[test]
fn abandoned_async_executor_releases_operation_slot_without_success() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let manager = Manager::default();
    let run = lease(&policy(&manager, "async"), 2, 32);
    let permission = permission();
    let mut future = Box::pin(run.execute_async(
        &permission,
        std::future::pending::<Result<((), usize), Error>>(),
    ));
    let waker = Waker::noop();
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(waker)),
        Poll::Pending
    ));
    assert_eq!(manager.operations()[0].state, OperationState::Running);
    drop(future);
    assert_eq!(
        manager.operations()[0].state,
        OperationState::DeliveryDenied(Error::Cancelled)
    );
    assert!(run.execute(&permission, || Ok(((), 0))).is_ok());
}

#[test]
fn async_revocation_after_execution_denies_result_delivery() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let manager = Manager::default();
    let policy = policy(&manager, "async");
    let run = lease(&policy, 1, 32);
    let permission = permission();
    let mut future = Box::pin(run.execute_async(&permission, async {
        policy.revoke();
        Ok(("secret", 6))
    }));
    let waker = Waker::noop();
    assert!(matches!(
        future.as_mut().poll(&mut Context::from_waker(waker)),
        Poll::Ready(Err(Error::Revoked))
    ));
    assert_eq!(
        manager.operations()[0].state,
        OperationState::DeliveryDenied(Error::Revoked)
    );
}
