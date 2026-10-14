use super::*;
fn profile(id: &str) -> SubscriptionProfile {
    serde_json::from_value(serde_json::json!({"id":id,"name":"Private source","url":"https://example.invalid/?token=canary",
        "enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":null,"updateIntervalSecs":60,
        "updatedAtUnixMs":1,"lastSyncAtUnixMs":1000,"lastError":null})).unwrap()
}
fn record(id: &str, status: AttemptStatus) -> AttemptRecord {
    AttemptRecord {
        subscription_id: id.into(),
        status,
        started_at_unix_ms: 70_000,
        finished_at_unix_ms: None,
        baseline_last_sync_at_unix_ms: Some(1000),
        confirmed_last_sync_at_unix_ms: None,
        next_eligible_at_unix_ms: 0,
        failures: 0,
        reason_code: None,
    }
}
#[test]
fn foreground_generations_stop_old_queue_and_coalesce_repeated_callbacks() {
    let scheduler = MobileSubscriptionScheduler::default();
    let epoch = scheduler.enter().unwrap().unwrap();
    assert!(scheduler.enter().unwrap().is_none());
    assert!(scheduler.current(epoch));
    scheduler.leave().unwrap();
    assert!(!scheduler.current(epoch));
    assert!(scheduler.enter().unwrap().is_none());
    let next = scheduler.finish(epoch).unwrap();
    assert!(scheduler.current(next));
    assert!(scheduler.finish(next).is_none());
    assert!(!scheduler.snapshot().unwrap().checking);
}
#[test]
fn restored_in_progress_is_unknown_and_not_automatically_replayed() {
    let profiles = vec![profile("s")];
    let mut checkpoint = Checkpoint::default();
    checkpoint
        .records
        .insert("s".into(), record("s", AttemptStatus::InProgress));
    reconcile(&mut checkpoint, &profiles);
    assert_eq!(checkpoint.records["s"].status, AttemptStatus::ResultUnknown);
    assert!(select_due(&profiles, &checkpoint, u64::MAX).is_empty());
    let mut confirmed = profiles;
    confirmed[0].last_sync_at_unix_ms = Some(90000);
    reconcile(&mut checkpoint, &confirmed);
    assert!(checkpoint.records.is_empty());
    assert!(select_due(&confirmed, &checkpoint, 90001).is_empty());
}
#[test]
fn due_selection_filters_owned_manual_disabled_and_invalid_intervals() {
    let mut profiles = vec![
        profile("due"),
        profile("fresh"),
        profile("disabled"),
        profile("manual"),
        profile("bad"),
    ];
    profiles[1].last_sync_at_unix_ms = Some(99_999);
    profiles[2].enabled = false;
    profiles[3].update_interval_secs = None;
    profiles[4].update_interval_secs = Some(0);
    let mut managed = profile("managed");
    managed.managed_source = Some(crate::models::subscription::ManagedSubscriptionSource {
        plugin_id: "plugin".into(),
        provider_id: "https://provider.example".into(),
        remote_subscription_id: "source".into(),
        revision: None,
        source_name: None,
    });
    profiles.push(managed);

    assert_eq!(
        select_due(&profiles, &Checkpoint::default(), 100_000)
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec!["due"]
    );
}
#[test]
fn durable_records_have_no_source_names_urls_tokens_or_material() {
    let mut checkpoint = Checkpoint::default();
    checkpoint
        .records
        .insert("s".into(), record("s", AttemptStatus::Failed));
    let encoded = serde_json::to_string(&checkpoint).unwrap();
    for value in ["canary", "https://", "Private source"] {
        assert!(!encoded.contains(value));
    }
    assert_eq!(
        safe_reason("https://secret/?token=canary"),
        "synchronization-failed"
    );
}
#[test]
fn unknown_records_are_never_evicted_to_create_an_unrecorded_request() {
    let mut checkpoint = Checkpoint::default();
    for i in 0..MAX_RECORDS {
        let id = format!("s{i}");
        checkpoint
            .records
            .insert(id.clone(), record(&id, AttemptStatus::ResultUnknown));
    }
    assert!(!prune(&mut checkpoint, "new"));
    assert_eq!(checkpoint.records.len(), MAX_RECORDS);
    checkpoint.records.get_mut("s0").unwrap().status = AttemptStatus::Failed;
    assert!(prune(&mut checkpoint, "new"));
    assert_eq!(checkpoint.records.len(), MAX_RECORDS - 1);
}
#[test]
fn failure_cycle_has_cooldown_and_never_uses_foreground_as_a_retry_timer() {
    assert_eq!(retry_delay_ms(1, 60), 60000);
    assert_eq!(retry_delay_ms(2, 60), 120000);
    assert_eq!(retry_delay_ms(4, 60), 900000);
    assert_eq!(retry_delay_ms(4, 3600), 3600000);
    let mut checkpoint = Checkpoint::default();
    let mut delayed = record("s", AttemptStatus::Failed);
    delayed.next_eligible_at_unix_ms = 200000;
    checkpoint.records.insert("s".into(), delayed);
    assert!(select_due(&[profile("s")], &checkpoint, 199999).is_empty());
}

#[test]
fn disabling_and_reenabling_intent_does_not_remove_uncertain_operation_barrier() {
    let mut source = profile("s");
    let mut checkpoint = Checkpoint::default();
    checkpoint
        .records
        .insert("s".into(), record("s", AttemptStatus::ResultUnknown));
    source.enabled = false;
    source.update_interval_secs = None;
    reconcile(&mut checkpoint, &[source.clone()]);
    assert_eq!(checkpoint.records.len(), 1);
    source.enabled = true;
    source.update_interval_secs = Some(60);
    assert!(select_due(&[source], &checkpoint, u64::MAX).is_empty());
}

#[test]
fn actual_private_storage_reopen_preserves_uncertainty_without_replaying() {
    use crate::services::app_database::storage::{self, Change};
    use std::{
        collections::BTreeSet,
        sync::{atomic::AtomicBool, Arc},
    };
    use znet_client_core::capability::{Budget, Manager, Permission};
    let access = || {
        let manager = Manager::default();
        let grants = BTreeSet::from([
            Permission::new("storage.read", "self"),
            Permission::new("storage.write", "self"),
        ]);
        let policy = manager
            .admit(
                "builtin.scheduler.mobile-subscriptions".into(),
                grants.clone(),
                grants.clone(),
                &grants,
            )
            .unwrap();
        policy.authorize(grants, Duration::from_secs(30)).unwrap();
        policy
            .begin(
                Budget {
                    calls: 2,
                    resource_bytes: 16384,
                    timeout: Duration::from_secs(10),
                },
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap()
    };
    let directory = tempfile::tempdir().unwrap();
    let mut checkpoint = Checkpoint::default();
    checkpoint
        .records
        .insert("s".into(), record("s", AttemptStatus::InProgress));
    let lease = access();
    let saved = storage::change(
        directory.path(),
        &lease,
        &[Change::Put {
            key: "retry-state".into(),
            expected: None,
            value: serde_json::to_vec(&checkpoint).unwrap(),
        }],
    )
    .unwrap()
    .take(&lease)
    .unwrap();
    assert_eq!(saved.len(), 1);
    drop(lease);
    let lease = access();
    let stored = storage::get(directory.path(), &lease, "retry-state")
        .unwrap()
        .take(&lease)
        .unwrap()
        .unwrap();
    let mut restored: Checkpoint = serde_json::from_slice(&stored.value).unwrap();
    assert!(valid_checkpoint(&restored));
    reconcile(&mut restored, &[profile("s")]);
    assert_eq!(restored.records["s"].status, AttemptStatus::ResultUnknown);
    assert!(select_due(&[profile("s")], &restored, u64::MAX).is_empty());
}
