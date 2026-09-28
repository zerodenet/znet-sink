use super::*;
use std::cell::Cell;

fn fixture() -> (ManagedSubscriptionSyncComplete, SubscriptionProfile) {
    let input = ManagedSubscriptionSyncComplete {
        plugin_id: "org.example.plugin".into(),
        provider_id: "https://panel.example.com".into(),
        remote_subscription_id: "primary".into(),
        revision: "rev-2".into(),
    };
    let (id, mut owner, _) = validate(input.clone()).unwrap();
    owner.revision = Some(input.revision.clone());
    let profile = serde_json::from_value(json!({
        "id": id, "name": "Plan", "url": input.provider_id,
        "enabled": true, "kernel": "zero", "format": "zero",
        "targetProxyConfigId": "existing-config", "managedSource": owner,
        "policySelections": {"Proxy": "Japan"}, "nodeCount": 19,
        "usedBytes": 375, "totalBytes": 1000, "expireAtUnixMs": 1800000000000u64,
        "updatedAtUnixMs": 10, "lastSyncAtUnixMs": 10, "lastError": "previous failure"
    }))
    .unwrap();
    (input, profile)
}

#[test]
fn unchanged_sync_updates_only_completion_fields_after_durable_save() {
    let (input, profile) = fixture();
    let (id, owner, revision) = validate(input).unwrap();
    let stored = Cell::new(false);
    let (_, updated) = commit(
        &[profile.clone()],
        &id,
        &owner,
        &revision,
        20,
        || Ok(()),
        |next| {
            assert_eq!(next[0].last_sync_at_unix_ms, Some(20));
            stored.set(true);
            Ok(())
        },
    )
    .unwrap();
    assert!(stored.get());
    let mut expected = profile;
    expected.last_sync_at_unix_ms = Some(20);
    expected.updated_at_unix_ms = 20;
    expected.last_error = None;
    assert_eq!(
        serde_json::to_value(updated).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
}

#[test]
fn unchanged_sync_rejects_missing_manual_foreign_and_superseded_profiles() {
    let (input, profile) = fixture();
    let (id, owner, revision) = validate(input).unwrap();
    let mut manual = profile.clone();
    manual.managed_source = None;
    let mut foreign = profile.clone();
    foreign.managed_source.as_mut().unwrap().plugin_id = "another-plugin".into();
    let mut superseded = profile.clone();
    superseded.managed_source.as_mut().unwrap().revision = Some("rev-3".into());
    for previous in [vec![], vec![manual], vec![foreign], vec![superseded]] {
        assert!(commit(
            &previous,
            &id,
            &owner,
            &revision,
            20,
            || Ok(()),
            |_| { panic!("invalid completion must never persist") }
        )
        .is_err());
    }
}

#[test]
fn unchanged_sync_does_not_commit_after_revocation_or_storage_failure() {
    let (input, profile) = fixture();
    let (id, owner, revision) = validate(input).unwrap();
    let previous = vec![profile];
    let snapshot = serde_json::to_value(&previous).unwrap();
    assert!(commit(
        &previous,
        &id,
        &owner,
        &revision,
        20,
        || Err(AppError::invalid_argument("revoked")),
        |_| panic!("revoked completion must never persist")
    )
    .is_err());
    assert!(commit(
        &previous,
        &id,
        &owner,
        &revision,
        20,
        || Ok(()),
        |_| Err(AppError::internal("storage failure"))
    )
    .is_err());
    assert_eq!(serde_json::to_value(previous).unwrap(), snapshot);
}

#[test]
fn unchanged_sync_requires_normalized_owner_and_nonempty_revision() {
    let (input, _) = fixture();
    for bad_revision in ["", " ", " rev-2", "rev-2\n"] {
        assert!(validate(ManagedSubscriptionSyncComplete {
            revision: bad_revision.into(),
            ..input.clone()
        })
        .is_err());
    }
    assert!(validate(ManagedSubscriptionSyncComplete {
        provider_id: "https://panel.example.com/path".into(),
        ..input.clone()
    })
    .is_err());
    assert!(validate(ManagedSubscriptionSyncComplete {
        remote_subscription_id: "../primary".into(),
        ..input
    })
    .is_err());
}
