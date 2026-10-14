use super::*;
use serde_json::json;
fn profile(id: &str) -> ProxyConfigProfile {
    ProxyConfigProfile {
        id: id.into(),
        name: id.into(),
        kernel: "zero".into(),
        format: "zero-json".into(),
        path: None,
        content: Some(
            json!({"inbounds":[],"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],"route":{"final":{"type":"direct"}}}),
        ),
        active: id == "target",
        managed_source: None,
        updated_at_unix_ms: 10,
        capabilities: Default::default(),
    }
}
fn source(id: &str, target: Option<&str>) -> SubscriptionProfile {
    serde_json::from_value(json!({"id":id,"name":id,"url":"https://provider.invalid/sub?token=ordinary-only","enabled":true,"kernel":"zero","format":"auto","targetProxyConfigId":target,"updatedAtUnixMs":10,"lastSyncAtUnixMs":9,"lastError":null})).unwrap()
}
fn baseline(dir: &Path) -> (Vec<ProxyConfigProfile>, Vec<SubscriptionProfile>) {
    let mut other = profile("managed");
    other.content = Some(json!({"protected-body":"other-managed-canary"}));
    other.managed_source = Some(
        serde_json::from_value(json!({"pluginId":"p","providerId":"s","remoteSubscriptionId":"r"}))
            .unwrap(),
    );
    let profiles = vec![profile("target"), other];
    let subs = vec![source("ordinary", Some("target"))];
    save_domain_data(dir, &profiles, &subs).unwrap();
    (profiles, subs)
}
fn input(profiles: &[ProxyConfigProfile], subs: &[SubscriptionProfile], id: &str) -> Prepare {
    let mut next_profile = profiles[0].clone();
    next_profile.updated_at_unix_ms = 15;
    let mut next_subscription = subs[0].clone();
    next_subscription.updated_at_unix_ms = 15;
    next_subscription.last_sync_at_unix_ms = Some(15);
    next_subscription.node_count = Some(1);
    Prepare {
        recovery_id: id.into(),
        expected_source: subs[0].clone(),
        expected_profile: Some(profiles[0].clone()),
        expected_graph_fingerprint: graph_fingerprint(profiles, subs).unwrap(),
        expected_rule_sets_fingerprint: fingerprint(&Vec::<String>::new()).unwrap(),
        expected_app_fingerprint: fingerprint(&json!({"before":"app"})).unwrap(),
        projected_app_fingerprint: fingerprint(&json!({"after":"app"})).unwrap(),
        effective_config_fingerprint: fingerprint(&next_profile.content).unwrap(),
        capture_plan_fingerprint: fingerprint(&json!({"capture":"plan"})).unwrap(),
        next_profile,
        next_subscription,
        created_at_unix_ms: 15,
    }
}
fn receipt() -> ConfirmedIdentity {
    ConfirmedIdentity {
        core_instance_id: "original-instance".into(),
        config_revision: 9007199254740993,
        persistence: "runtime_only".into(),
        capture_validated: true,
    }
}
#[test]
fn prepare_barrier_candidate_and_source_are_atomic_and_reopen_without_other_managed_material() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
    assert_eq!(prepared.barrier_source.updated_at_unix_ms, 15);
    assert_eq!(prepared.barrier_source.last_sync_at_unix_ms, Some(9));
    assert_eq!(
        prepared.barrier_source.last_error.as_deref(),
        Some(crate::services::subscription_reconciliation::UNKNOWN_APPLY_MESSAGE)
    );
    let record = load(root.path(), "operation").unwrap().unwrap();
    assert_eq!(record.stage, Stage::Prepared);
    assert_eq!(
        record.expected_source_fingerprint,
        fingerprint(&prepared.barrier_source).unwrap()
    );
    assert_eq!(record.barrier_source_updated_at_unix_ms, 15);
    assert_eq!(record.published_source_updated_at_unix_ms, 16);
    assert_eq!(
        record
            .next_subscription
            .as_ref()
            .unwrap()
            .last_sync_at_unix_ms,
        Some(15)
    );
    let graph = load_domain_data(root.path()).unwrap();
    assert_eq!(
        fingerprint(&graph.subscriptions[0]).unwrap(),
        record.expected_source_fingerprint
    );
    assert_eq!(
        fingerprint(&graph.proxy_configs).unwrap(),
        fingerprint(&profiles).unwrap()
    );
    let connection = db_open(root.path()).unwrap();
    let(meta,candidate):(String,String)=connection.query_row("SELECT metadata_json,candidate_json FROM ordinary_subscription_recovery WHERE recovery_id='operation'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    assert!(!meta.contains("ordinary-only"));
    assert!(!candidate.contains("other-managed-canary"));
    assert!(!format!("{record:?}").contains("ordinary-only"));
    let metadata = list_pending_metadata(root.path()).unwrap();
    assert_eq!(metadata.len(), 1);
    assert!(metadata[0].candidate_present);
    assert_eq!(metadata[0].stage, Stage::Prepared);
}
#[test]
fn only_confirmed_authority_can_publish_and_projection_completion_keeps_barrier_until_final_tx() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
    assert!(publish_local(root.path(), "operation", prepared.record.version).is_err());
    let pending = mark_apply_pending(root.path(), "operation", prepared.record.version).unwrap();
    for invalid in [
        ConfirmedIdentity {
            persistence: "source_file".into(),
            ..receipt()
        },
        ConfirmedIdentity {
            capture_validated: false,
            ..receipt()
        },
    ] {
        assert!(confirm(root.path(), "operation", pending.version, invalid).is_err());
    }
    let confirmed = confirm(root.path(), "operation", pending.version, receipt()).unwrap();
    assert_eq!(
        load(root.path(), "operation")
            .unwrap()
            .unwrap()
            .confirmed_identity
            .unwrap()
            .config_revision,
        9007199254740993
    );
    let published = publish_local(root.path(), "operation", confirmed.version).unwrap();
    assert_eq!(published.stage, Stage::LocalCommitted);
    assert_eq!(
        prepared.record.preparation_graph_fingerprint,
        graph_fingerprint(&profiles, &subs).unwrap()
    );
    assert_ne!(
        prepared.record.preparation_graph_fingerprint,
        prepared.record.expected_graph_fingerprint
    );
    assert_eq!(
        published.local_published_graph_fingerprint,
        published.published_graph_fingerprint
    );
    let lightweight = load_metadata(root.path(), "operation").unwrap().unwrap();
    assert_eq!(
        lightweight.preparation_graph_fingerprint,
        prepared.record.preparation_graph_fingerprint
    );
    assert_eq!(
        lightweight.local_published_graph_fingerprint,
        published.published_graph_fingerprint
    );
    let graph = load_domain_data(root.path()).unwrap();
    assert!(graph.subscriptions[0].last_error.is_some());
    assert_eq!(graph.subscriptions[0].last_sync_at_unix_ms, Some(9));
    assert_eq!(graph.subscriptions[0].updated_at_unix_ms, 15);
    assert_eq!(graph.proxy_configs[0].updated_at_unix_ms, 15);
    assert!(metadata_for_subscription(root.path(), "ordinary")
        .unwrap()
        .is_some());
    let completed = complete(root.path(), "operation", published.version).unwrap();
    assert_eq!(completed.stage, Stage::Complete);
    assert_eq!(
        completed.local_published_graph_fingerprint,
        published.published_graph_fingerprint
    );
    assert_ne!(
        completed.local_published_graph_fingerprint,
        completed.published_graph_fingerprint
    );
    let lightweight = load_metadata(root.path(), "operation").unwrap().unwrap();
    assert_eq!(
        lightweight.local_published_graph_fingerprint,
        published.published_graph_fingerprint
    );
    assert_eq!(
        lightweight.expected_rule_sets_fingerprint,
        completed.expected_rule_sets_fingerprint
    );
    assert!(completed.next_profile.is_none());
    assert!(completed.next_subscription.is_none());
    let graph = load_domain_data(root.path()).unwrap();
    assert!(graph.subscriptions[0].last_error.is_none());
    assert_eq!(graph.subscriptions[0].last_sync_at_unix_ms, Some(15));
    assert_eq!(graph.subscriptions[0].updated_at_unix_ms, 16);
    let repeated = complete(root.path(), "operation", completed.version).unwrap();
    assert_eq!(repeated.version, completed.version);
    assert!(metadata_for_subscription(root.path(), "ordinary")
        .unwrap()
        .is_none());
}
#[test]
fn stale_graph_record_version_and_duplicate_source_cannot_change_domains_or_overwrite_evidence() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
    assert!(prepare(root.path(), input(&profiles, &subs, "duplicate")).is_err());
    assert!(mark_apply_pending(root.path(), "operation", prepared.record.version + 1).is_err());
    let pending = mark_apply_pending(root.path(), "operation", prepared.record.version).unwrap();
    let confirmed = confirm(root.path(), "operation", pending.version, receipt()).unwrap();
    let mut graph = load_domain_data(root.path()).unwrap();
    graph.proxy_configs[1].name = "unrelated changed".into();
    save_domain_data(root.path(), &graph.proxy_configs, &graph.subscriptions).unwrap();
    let before = graph_fingerprint(&graph.proxy_configs, &graph.subscriptions).unwrap();
    assert!(publish_local(root.path(), "operation", confirmed.version).is_err());
    assert_eq!(
        load(root.path(), "operation").unwrap().unwrap().stage,
        Stage::RuntimeConfirmed
    );
    let after = load_domain_data(root.path()).unwrap();
    assert_eq!(
        graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
        before
    );
}
#[test]
fn interrupted_stages_never_replay_and_invalid_candidate_or_unknown_records_do_not_silently_reset()
{
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
    let pending = mark_apply_pending(root.path(), "operation", prepared.record.version).unwrap();
    assert_eq!(
        pending_for_subscription(root.path(), "ordinary")
            .unwrap()
            .unwrap()
            .stage,
        Stage::ApplyPending
    );
    assert!(complete(root.path(), "operation", pending.version).is_err());
    let connection = db_open(root.path()).unwrap();
    connection.execute("UPDATE ordinary_subscription_recovery SET candidate_json='{}' WHERE recovery_id='operation'",[]).unwrap();
    assert!(load(root.path(), "operation").is_err());
    assert_eq!(
        list_pending_metadata(root.path()).unwrap()[0].stage,
        Stage::ApplyPending
    );
    let source = load_domain_data(root.path())
        .unwrap()
        .subscriptions
        .remove(0);
    assert!(source.last_error.is_some());
    connection.execute("UPDATE ordinary_subscription_recovery SET metadata_json='{}' WHERE recovery_id='operation'",[]).unwrap();
    assert!(list_pending_metadata(root.path()).is_err());
    assert!(load(root.path(), "operation").is_err());
}
#[test]
fn managed_source_and_overlarge_candidate_reject_before_barrier_or_stage_and_errors_are_redacted() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    let mut candidate = input(&profiles, &subs, "operation");
    candidate.next_profile.managed_source = profiles[1].managed_source.clone();
    assert!(prepare(root.path(), candidate).is_err());
    let mut candidate = input(&profiles, &subs, "too-large");
    candidate.next_profile.name = "candidate-secret-canary".repeat(MAX_CANDIDATE_BYTES / 10);
    let error = prepare(root.path(), candidate).err().unwrap();
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("candidate-secret-canary"));
    assert!(list_pending_metadata(root.path()).unwrap().is_empty());
    let after = load_domain_data(root.path()).unwrap();
    assert_eq!(
        graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
        graph_fingerprint(&profiles, &subs).unwrap()
    );
}
#[test]
fn schema_two_upgrade_preserves_relationships_and_existing_capability_rows() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    let connection = db_open(root.path()).unwrap();
    connection.execute("INSERT INTO capability_values(owner,key,value,revision) VALUES('owner','key',x'6f6c64',1)",[]).unwrap();
    connection
        .execute("DROP TABLE ordinary_subscription_recovery", [])
        .unwrap();
    connection
        .execute("DROP TABLE ordinary_recovery_clock", [])
        .unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    drop(connection);
    let after = load_domain_data(root.path()).unwrap();
    assert_eq!(
        graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
        graph_fingerprint(&profiles, &subs).unwrap()
    );
    let connection = db_open(root.path()).unwrap();
    let version: i32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 3);
    let bytes: Vec<u8> = connection
        .query_row(
            "SELECT value FROM capability_values WHERE owner='owner'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(bytes, b"old");
    assert!(list_pending_metadata(root.path()).unwrap().is_empty());
}
#[test]
fn monotonic_source_versions_reject_overflow_before_any_submission() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, mut subs) = baseline(root.path());
    subs[0].updated_at_unix_ms = u64::MAX;
    save_domain_data(root.path(), &profiles, &subs).unwrap();
    let before = graph_fingerprint(&profiles, &subs).unwrap();
    assert!(prepare(root.path(), input(&profiles, &subs, "overflow")).is_err());
    assert!(list_pending_metadata(root.path()).unwrap().is_empty());
    let after = load_domain_data(root.path()).unwrap();
    assert_eq!(
        graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
        before
    );
}

#[test]
fn thirty_two_pending_limit_is_atomic_and_never_evicts_an_unknown_operation() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, _) = baseline(root.path());
    let sources = (0..33)
        .map(|i| source(&format!("source-{i}"), None))
        .collect::<Vec<_>>();
    save_domain_data(root.path(), &profiles, &sources).unwrap();
    for i in 0..33 {
        let graph = load_domain_data(root.path()).unwrap();
        let current = graph
            .subscriptions
            .iter()
            .find(|source| source.id == format!("source-{i}"))
            .unwrap()
            .clone();
        let mut candidate = input(
            &graph.proxy_configs,
            std::slice::from_ref(&current),
            &format!("operation-{i}"),
        );
        candidate.expected_graph_fingerprint =
            graph_fingerprint(&graph.proxy_configs, &graph.subscriptions).unwrap();
        candidate.expected_profile = None;
        candidate.next_profile.id = format!("new-target-{i}");
        candidate.next_profile.active = false;
        candidate.next_subscription.target_proxy_config_id =
            Some(candidate.next_profile.id.clone());
        if i < 32 {
            prepare(root.path(), candidate).unwrap();
        } else {
            let before = graph_fingerprint(&graph.proxy_configs, &graph.subscriptions).unwrap();
            assert!(prepare(root.path(), candidate).is_err());
            let after = load_domain_data(root.path()).unwrap();
            assert_eq!(
                graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
                before
            );
            assert!(load(root.path(), "operation-32").unwrap().is_none());
        }
    }
    assert_eq!(list_pending_metadata(root.path()).unwrap().len(), 32);
    assert!(has_pending(root.path(), "source-0").unwrap());
    assert!(load(root.path(), "operation-0").unwrap().is_some());
}
#[test]
fn cumulative_material_budget_uses_same_transaction_path_with_fixture_limits() {
    assert_eq!(MAX_LEDGER_BYTES, 64 * 1024 * 1024);
    assert_eq!(MAX_CANDIDATE_BYTES, 16 * 1024 * 1024);
    let root = tempfile::tempdir().unwrap();
    let (profiles, _) = baseline(root.path());
    let sources = vec![
        source("first", Some("target")),
        source("second", Some("target")),
    ];
    save_domain_data(root.path(), &profiles, &sources).unwrap();
    let mut candidate = input(
        &profiles,
        std::slice::from_ref(&sources[0]),
        "first-operation",
    );
    candidate.expected_graph_fingerprint = graph_fingerprint(&profiles, &sources).unwrap();
    prepare(root.path(), candidate).unwrap();
    let connection = db_open(root.path()).unwrap();
    let used:i64=connection.query_row("SELECT sum(length(CAST(recovery_id AS BLOB))+length(CAST(subscription_id AS BLOB))+length(CAST(metadata_json AS BLOB))+coalesce(length(CAST(candidate_json AS BLOB)),0)) FROM ordinary_subscription_recovery",[],|row|row.get(0)).unwrap();
    drop(connection);
    let graph = load_domain_data(root.path()).unwrap();
    let mut candidate = input(
        &graph.proxy_configs,
        std::slice::from_ref(&graph.subscriptions[1]),
        "second-operation",
    );
    candidate.expected_graph_fingerprint =
        graph_fingerprint(&graph.proxy_configs, &graph.subscriptions).unwrap();
    let before = graph_fingerprint(&graph.proxy_configs, &graph.subscriptions).unwrap();
    assert!(prepare_with_limits(
        root.path(),
        candidate,
        BudgetLimits {
            pending: 32,
            bytes: used + 1
        }
    )
    .is_err());
    assert!(load(root.path(), "second-operation").unwrap().is_none());
    assert_eq!(list_pending_metadata(root.path()).unwrap().len(), 1);
    let after = load_domain_data(root.path()).unwrap();
    assert_eq!(
        graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
        before
    );
}
#[test]
fn complete_keeps_only_thirty_two_compact_tombstones_and_old_id_cannot_be_replayed() {
    let root = tempfile::tempdir().unwrap();
    baseline(root.path());
    for i in 0..33 {
        let graph = load_domain_data(root.path()).unwrap();
        let prepared = prepare(
            root.path(),
            input(
                &graph.proxy_configs,
                &graph.subscriptions,
                &format!("operation-{i}"),
            ),
        )
        .unwrap();
        let pending = mark_apply_pending(
            root.path(),
            &prepared.record.recovery_id,
            prepared.record.version,
        )
        .unwrap();
        let confirmed = confirm(
            root.path(),
            &pending.recovery_id,
            pending.version,
            receipt(),
        )
        .unwrap();
        let published =
            publish_local(root.path(), &confirmed.recovery_id, confirmed.version).unwrap();
        complete(root.path(), &published.recovery_id, published.version).unwrap();
    }
    let connection = db_open(root.path()).unwrap();
    let (count, bodies): (i64, i64) = connection
        .query_row(
            "SELECT count(*),sum(candidate_json IS NOT NULL) FROM ordinary_subscription_recovery",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 32);
    assert_eq!(bodies, 0);
    assert!(load(root.path(), "operation-0").unwrap().is_none());
    assert!(publish_local(root.path(), "operation-0", 1).is_err());
    assert_eq!(
        latest_for_subscription(root.path(), "ordinary")
            .unwrap()
            .unwrap()
            .recovery_id,
        "operation-32"
    );
}
#[test]
fn same_timestamp_source_and_target_edits_in_database_are_real_cas_conflicts() {
    for source_edit in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let (profiles, subs) = baseline(root.path());
        let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
        let pending =
            mark_apply_pending(root.path(), "operation", prepared.record.version).unwrap();
        let confirmed = confirm(root.path(), "operation", pending.version, receipt()).unwrap();
        let mut graph = load_domain_data(root.path()).unwrap();
        if source_edit {
            graph.subscriptions[0].user_agent = Some("changed".into());
        } else {
            graph.proxy_configs[0].content.as_mut().unwrap()["route"]["final"] =
                json!({"type":"reject"});
        }
        save_domain_data(root.path(), &graph.proxy_configs, &graph.subscriptions).unwrap();
        let expected = graph_fingerprint(&graph.proxy_configs, &graph.subscriptions).unwrap();
        assert!(publish_local(root.path(), "operation", confirmed.version).is_err());
        let after = load_domain_data(root.path()).unwrap();
        assert_eq!(
            graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
            expected
        );
        assert_eq!(
            load(root.path(), "operation").unwrap().unwrap().stage,
            Stage::RuntimeConfirmed
        );
    }
}
#[test]
fn sqlite_trigger_fault_rolls_back_source_profile_and_ledger_at_each_publication_boundary() {
    for fail_stage in ["prepare", "publish", "complete"] {
        let root = tempfile::tempdir().unwrap();
        let (profiles, subs) = baseline(root.path());
        let connection = db_open(root.path()).unwrap();
        if fail_stage == "prepare" {
            connection.execute_batch("CREATE TRIGGER deny_source BEFORE UPDATE ON subscriptions BEGIN SELECT RAISE(ABORT,'trigger-secret-canary');END;").unwrap();
            let error = prepare(root.path(), input(&profiles, &subs, "operation"))
                .err()
                .unwrap();
            assert!(!serde_json::to_string(&error)
                .unwrap()
                .contains("trigger-secret-canary"));
            assert!(load(root.path(), "operation").unwrap().is_none());
            let after = load_domain_data(root.path()).unwrap();
            assert_eq!(
                graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
                graph_fingerprint(&profiles, &subs).unwrap()
            );
            continue;
        }
        let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
        let pending =
            mark_apply_pending(root.path(), "operation", prepared.record.version).unwrap();
        let confirmed = confirm(root.path(), "operation", pending.version, receipt()).unwrap();
        let record = if fail_stage == "complete" {
            publish_local(root.path(), "operation", confirmed.version).unwrap()
        } else {
            confirmed
        };
        let before = load_domain_data(root.path()).unwrap();
        let hash = graph_fingerprint(&before.proxy_configs, &before.subscriptions).unwrap();
        connection.execute_batch("CREATE TRIGGER deny_source BEFORE UPDATE ON subscriptions BEGIN SELECT RAISE(ABORT,'trigger-secret-canary');END;").unwrap();
        let error = if fail_stage == "complete" {
            complete(root.path(), "operation", record.version)
        } else {
            publish_local(root.path(), "operation", record.version)
        }
        .unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("trigger-secret-canary"));
        let after = load_domain_data(root.path()).unwrap();
        assert_eq!(
            graph_fingerprint(&after.proxy_configs, &after.subscriptions).unwrap(),
            hash
        );
        let retained = load(root.path(), "operation").unwrap().unwrap();
        assert_eq!(retained.stage, record.stage);
        assert_eq!(retained.version, record.version);
        assert!(after.subscriptions[0].last_error.is_some());
    }
}
#[test]
fn candidate_digest_tampering_and_invalid_ids_cannot_create_executable_records() {
    let root = tempfile::tempdir().unwrap();
    let (profiles, subs) = baseline(root.path());
    for id in [
        " whitespace",
        "-first",
        "id/segment",
        "id?token=secret-canary",
    ] {
        let error = prepare(root.path(), input(&profiles, &subs, id))
            .err()
            .unwrap();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("secret-canary"));
    }
    let prepared = prepare(root.path(), input(&profiles, &subs, "operation")).unwrap();
    let connection = db_open(root.path()).unwrap();
    let json:String=connection.query_row("SELECT candidate_json FROM ordinary_subscription_recovery WHERE recovery_id='operation'",[],|row|row.get(0)).unwrap();
    let mut candidate: serde_json::Value = serde_json::from_str(&json).unwrap();
    candidate["profile"]["name"] = json!("changed-secret-canary");
    connection.execute("UPDATE ordinary_subscription_recovery SET candidate_json=?1 WHERE recovery_id='operation'",[candidate.to_string()]).unwrap();
    assert!(load(root.path(), "operation").is_err());
    assert!(mark_apply_pending(root.path(), "operation", prepared.record.version).is_err());
    assert!(has_pending(root.path(), "ordinary").unwrap());
    assert!(load_domain_data(root.path()).unwrap().subscriptions[0]
        .last_error
        .is_some());
}
