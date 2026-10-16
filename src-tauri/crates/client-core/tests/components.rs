use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_core::capability::*;
fn grants() -> BTreeSet<Permission> {
    BTreeSet::from([Permission::new("network.get", "https://example.org")])
}
fn admit(manager: &Manager, digest: &str) -> Result<Policy, Error> {
    manager.admit_component(
        "plugin/main".into(),
        digest.into(),
        grants(),
        grants(),
        &grants(),
    )
}
fn begin(policy: &Policy) -> Result<Lease, Error> {
    policy.begin(
        Budget {
            calls: 2,
            resource_bytes: 128,
            timeout: Duration::from_secs(1),
        },
        Arc::new(AtomicBool::new(false)),
    )
}
#[test]
fn duplicate_admission_shares_grants_busy_state_and_revocation() {
    let manager = Manager::default();
    let first = admit(&manager, "digest").unwrap();
    assert!(matches!(begin(&first), Err(Error::Disabled)));
    first.authorize(grants(), Duration::from_secs(60)).unwrap();
    let run = begin(&first).unwrap();
    let second = admit(&manager, "digest").unwrap();
    assert!(matches!(begin(&second), Err(Error::Busy)));
    assert!(manager.revoke_component("plugin/main"));
    assert_eq!(run.check(None), Err(Error::Revoked));
    drop(run);
    assert!(matches!(begin(&second), Err(Error::Disabled)));
    assert!(!manager.component_snapshots()[0].enabled);
}
#[test]
fn replacement_retires_old_handles_and_waits_for_the_old_invocation() {
    let manager = Manager::default();
    let old = admit(&manager, "old").unwrap();
    old.authorize(grants(), Duration::from_secs(60)).unwrap();
    let run = begin(&old).unwrap();
    assert!(matches!(admit(&manager, "new"), Err(Error::Busy)));
    assert_eq!(run.check(None), Err(Error::Revoked));
    assert_eq!(
        old.authorize(grants(), Duration::from_secs(60)),
        Err(Error::Revoked)
    );
    drop(run);
    let new = admit(&manager, "new").unwrap();
    assert!(matches!(begin(&new), Err(Error::Disabled)));
    new.authorize(grants(), Duration::from_secs(60)).unwrap();
    assert!(begin(&new).is_ok());
    assert_eq!(manager.component_snapshots()[0].identity, "new");
}
#[test]
fn removal_and_reinstall_never_resurrect_old_authority_or_resources() {
    let manager = Manager::default();
    let old = admit(&manager, "same").unwrap();
    old.authorize(grants(), Duration::from_secs(60)).unwrap();
    let run = begin(&old).unwrap();
    let resource = run
        .execute(grants().first().unwrap(), || Ok(("canary", 6)))
        .unwrap();
    assert!(manager.remove_component("plugin/main"));
    assert!(matches!(admit(&manager, "same"), Err(Error::Busy)));
    assert_eq!(resource.take(&run), Err(Error::Revoked));
    drop(run);
    let new = admit(&manager, "same").unwrap();
    assert!(matches!(begin(&new), Err(Error::Disabled)));
    assert_eq!(
        old.authorize(grants(), Duration::from_secs(60)),
        Err(Error::Revoked)
    );
}
#[test]
fn invalid_replacement_does_not_revoke_valid_component() {
    let manager = Manager::default();
    let old = admit(&manager, "old").unwrap();
    old.authorize(grants(), Duration::from_secs(60)).unwrap();
    assert!(matches!(
        manager.admit_component(
            "plugin/main".into(),
            "new".into(),
            grants(),
            grants(),
            &BTreeSet::new()
        ),
        Err(Error::AdmissionDenied)
    ));
    assert!(begin(&old).is_ok());
}
#[test]
fn shutdown_retires_existing_handles_and_denies_future_admission() {
    let manager = Manager::default();
    let old = admit(&manager, "old").unwrap();
    old.authorize(grants(), Duration::from_secs(60)).unwrap();
    let run = begin(&old).unwrap();
    manager.shutdown_components();
    assert_eq!(run.check(None), Err(Error::Revoked));
    assert!(matches!(admit(&manager, "new"), Err(Error::Disabled)));
    assert_eq!(
        old.authorize(grants(), Duration::from_secs(60)),
        Err(Error::Revoked)
    );
}

#[test]
fn stale_authorization_review_cannot_undo_revocation_upgrade_or_reinstall() {
    let manager = Manager::default();
    let _old = admit(&manager, "old").unwrap();
    let review = manager.component_snapshots()[0].clone();
    manager.revoke_component(&review.key);
    assert_eq!(
        manager.authorize_component(&review, grants(), Duration::from_secs(60)),
        Err(Error::Revoked)
    );
    let fresh = manager.component_snapshots()[0].clone();
    manager
        .authorize_component(&fresh, grants(), Duration::from_secs(60))
        .unwrap();
    let _new = admit(&manager, "new").unwrap();
    assert_eq!(
        manager.authorize_component(&fresh, grants(), Duration::from_secs(60)),
        Err(Error::Revoked)
    );
    let before_remove = manager.component_snapshots()[0].clone();
    manager.remove_component(&before_remove.key);
    let _reinstalled = admit(&manager, "new").unwrap();
    assert_eq!(
        manager.authorize_component(&before_remove, grants(), Duration::from_secs(60)),
        Err(Error::Revoked)
    );
    assert!(!manager.component_snapshots()[0].enabled);
}

#[test]
fn retired_completed_entries_do_not_exhaust_registry_capacity() {
    let manager = Manager::default();
    for index in 0..100 {
        let key = format!("plugin/{index}");
        let policy = manager
            .admit_component(key.clone(), "digest".into(), grants(), grants(), &grants())
            .unwrap();
        policy.authorize(grants(), Duration::from_secs(60)).unwrap();
        let run = begin(&policy).unwrap();
        manager.remove_component(&key);
        drop(run);
    }
    assert!(manager.component_snapshots().len() <= 1);
}

#[test]
fn host_can_restore_durable_component_consent_without_expiring_an_operation_policy() {
    let manager = Manager::default();
    let _policy = admit(&manager, "digest").unwrap();
    let review = manager.component_snapshots()[0].clone();
    manager
        .authorize_component_persistent(&review, grants())
        .unwrap();
    let restored = manager.component_snapshots()[0].clone();
    assert!(restored.enabled);
    manager
        .authorize_component_persistent(&restored, grants())
        .unwrap();
    assert_eq!(manager.component_snapshots()[0].revision, restored.revision);
    assert!(begin(&admit(&manager, "digest").unwrap()).is_ok());
}

#[test]
fn reviewed_begin_cannot_reacquire_consent_after_stop_and_restart() {
    let manager = Manager::default();
    let policy = admit(&manager, "same").unwrap();
    policy.authorize(grants(), Duration::from_secs(60)).unwrap();
    let reviewed = manager.component_snapshots()[0].revision;
    let budget = Budget {
        calls: 1,
        resource_bytes: 128,
        timeout: Duration::from_secs(1),
    };
    let original = policy
        .begin_at_revision(budget.clone(), Arc::new(AtomicBool::new(false)), reviewed)
        .unwrap();
    policy.revoke();
    policy.authorize(grants(), Duration::from_secs(60)).unwrap();
    assert_eq!(original.check(None), Err(Error::Revoked));
    drop(original);
    assert!(matches!(
        policy.begin_at_revision(budget.clone(), Arc::new(AtomicBool::new(false)), reviewed),
        Err(Error::Revoked)
    ));
    let latest = manager.component_snapshots()[0].revision;
    assert!(policy
        .begin_at_revision(budget, Arc::new(AtomicBool::new(false)), latest)
        .is_ok());
}

#[test]
fn queued_execution_activates_original_wall_budget_once_without_new_consent() {
    let manager = Manager::default();
    let policy = admit(&manager, "same").unwrap();
    policy.authorize(grants(), Duration::from_secs(60)).unwrap();
    let revision = manager.component_snapshots()[0].revision;
    let lease = policy
        .begin_queued(
            Budget {
                calls: 1,
                resource_bytes: 128,
                timeout: Duration::from_millis(20),
            },
            Arc::new(AtomicBool::new(false)),
            Some(revision),
            Duration::from_secs(1),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(lease.check(None), Ok(()));
    lease.activate_queued_execution().unwrap();
    assert_eq!(
        lease.activate_queued_execution(),
        Err(Error::InvalidRequest)
    );
    assert_eq!(manager.component_snapshots()[0].revision, revision);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(lease.check(None), Err(Error::Deadline));
}
#[test]
fn queue_expiry_and_revocation_cannot_be_renewed_by_activation() {
    let manager = Manager::default();
    let policy = admit(&manager, "same").unwrap();
    policy.authorize(grants(), Duration::from_secs(60)).unwrap();
    let budget = Budget {
        calls: 1,
        resource_bytes: 128,
        timeout: Duration::from_secs(1),
    };
    let lease = policy
        .begin_queued(
            budget.clone(),
            Arc::new(AtomicBool::new(false)),
            None,
            Duration::from_millis(5),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(lease.activate_queued_execution(), Err(Error::Deadline));
    drop(lease);
    let lease = policy
        .begin_queued(
            budget,
            Arc::new(AtomicBool::new(false)),
            None,
            Duration::from_secs(1),
        )
        .unwrap();
    policy.revoke();
    policy.authorize(grants(), Duration::from_secs(60)).unwrap();
    assert_eq!(lease.activate_queued_execution(), Err(Error::Revoked));
}
