use super::*;
use serde_json::json;
pub(crate) fn fixture() -> (tempfile::TempDir, AppState, Record) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let profile = ProxyConfigProfile {
        id: "target".into(),
        name: "old".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(
            json!({"inbounds":[],"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],"route":{"final":{"type":"direct"}}}),
        ),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: Default::default(),
    };
    let source: SubscriptionProfile = serde_json::from_value(json!({
        "id":"source","name":"Source","url":"https://provider.invalid/?token=private-ordinary",
        "enabled":true,"kernel":"zero","format":"auto","targetProxyConfigId":"target",
        "updatedAtUnixMs":1,"lastSyncAtUnixMs":1,"lastError":null,
    }))
    .unwrap();
    *state.proxy_configs().lock().unwrap() = vec![profile.clone()];
    *state.subscriptions().lock().unwrap() = vec![source.clone()];
    state
        .app_config()
        .lock()
        .unwrap()
        .local_proxy
        .source_proxy_config_id = Some("target".into());
    super::super::app_database::save_domain_data(dir.path(), &[profile.clone()], &[source.clone()])
        .unwrap();
    let mut next_profile = profile;
    next_profile.name = "confirmed candidate".into();
    next_profile.updated_at_unix_ms = 2;
    let mut next_source = source.clone();
    next_source.node_count = Some(7);
    next_source.last_sync_at_unix_ms = Some(2);
    next_source.updated_at_unix_ms = 2;
    let app_path = dir.path().join("app-config.json");
    let initial_app = state.app_config().lock().unwrap().clone();
    super::super::app_config_store::save(&app_path, &initial_app).unwrap();
    *state.app_config().lock().unwrap() =
        super::super::app_config_store::load_or_default(&app_path).unwrap();
    super::super::domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    let expected_app = state.app_config().lock().unwrap().clone();
    let rules_fingerprint = ledger::fingerprint(&*state.rule_sets().lock().unwrap()).unwrap();
    let prepared = prepare_at(
        &state,
        dir.path(),
        &source,
        next_profile,
        next_source,
        &json!({"effective":"known"}),
        &json!({"capture":"known"}),
        &expected_app,
        &rules_fingerprint,
    )
    .unwrap();
    let dispatched =
        ledger::mark_apply_pending(dir.path(), &prepared.recovery_id, prepared.version).unwrap();
    let confirmed = ledger::confirm(
        dir.path(),
        &dispatched.recovery_id,
        dispatched.version,
        ledger::ConfirmedIdentity {
            core_instance_id: "original-core".into(),
            config_revision: 9_007_199_254_740_993,
            persistence: "runtime_only".into(),
            capture_validated: true,
        },
    )
    .unwrap();
    (dir, state, confirmed)
}
fn reopen_state(dir: &Path, previous: &AppState) -> AppState {
    let state = AppState::default();
    let _ = previous;
    *state.app_config().lock().unwrap() =
        super::super::app_config_store::load_or_default(&dir.join("app-config.json")).unwrap();
    let graph = super::super::app_database::load_domain_data(dir).unwrap();
    *state.proxy_configs().lock().unwrap() = graph.proxy_configs;
    *state.subscriptions().lock().unwrap() = graph.subscriptions;
    state
}
#[test]
fn confirmed_receipt_survives_real_sqlite_reopen_and_only_local_helper_publication() {
    let (dir, original, record) = fixture();
    let state = reopen_state(dir.path(), &original);
    let durable = ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap();
    let source = publish_at(
        &state,
        dir.path(),
        &dir.path().join("app-config.json"),
        durable,
        identity(&record).unwrap(),
        Some(record.barrier_source_updated_at_unix_ms),
    )
    .unwrap();
    assert_eq!(source.node_count, Some(7));
    assert_eq!(source.last_sync_at_unix_ms, Some(2));
    assert!(source.last_error.is_none());
    assert!(source.updated_at_unix_ms > record.barrier_source_updated_at_unix_ms);
    assert!(state
        .app_config()
        .lock()
        .unwrap()
        .local_proxy
        .source_proxy_config_id
        .is_none());
    let stored = ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.stage, Stage::Complete);
    assert!(stored.next_profile.is_none() && stored.next_subscription.is_none());
}
#[test]
fn projection_failure_preserves_unknown_then_reopened_local_phase_can_finish() {
    let (dir, state, record) = fixture();
    let record = ledger::publish_local(dir.path(), &record.recovery_id, record.version).unwrap();
    let blocked = dir.path().join("blocked-config");
    std::fs::create_dir(&blocked).unwrap();
    assert!(publish_at(
        &state,
        dir.path(),
        &blocked,
        record.clone(),
        identity(&record).unwrap(),
        None
    )
    .is_err());
    let stored = ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.stage, Stage::LocalCommitted);
    assert_eq!(
        state.subscriptions().lock().unwrap()[0].updated_at_unix_ms,
        record.barrier_source_updated_at_unix_ms
    );
    assert_eq!(
        state.subscriptions().lock().unwrap()[0]
            .last_error
            .as_deref(),
        Some(crate::services::subscription_reconciliation::UNKNOWN_APPLY_MESSAGE)
    );
    assert_eq!(state.proxy_configs().lock().unwrap()[0].name, "old");
    let reopened = reopen_state(dir.path(), &state);
    let source = publish_at(
        &reopened,
        dir.path(),
        &dir.path().join("app-config.json"),
        stored,
        identity(&record).unwrap(),
        Some(record.barrier_source_updated_at_unix_ms),
    )
    .unwrap();
    assert!(source.last_error.is_none());
    assert_eq!(source.last_sync_at_unix_ms, Some(2));
}
#[test]
fn completed_same_id_and_original_preview_version_are_idempotent_without_new_write() {
    let (dir, state, record) = fixture();
    let path = dir.path().join("app-config.json");
    let first = publish_at(
        &state,
        dir.path(),
        &path,
        record.clone(),
        identity(&record).unwrap(),
        None,
    )
    .unwrap();
    let completed = ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let again = publish_at(
        &state,
        dir.path(),
        &path,
        completed.clone(),
        identity(&record).unwrap(),
        Some(record.barrier_source_updated_at_unix_ms),
    )
    .unwrap();
    assert_eq!(
        ledger::fingerprint(&first).unwrap(),
        ledger::fingerprint(&again).unwrap()
    );
    assert_eq!(
        ledger::load(dir.path(), &record.recovery_id)
            .unwrap()
            .unwrap()
            .version,
        completed.version
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
}
#[test]
fn same_timestamp_source_material_and_settings_changes_cannot_resolve_old_candidate() {
    for change in 0..3 {
        let (dir, state, record) = fixture();
        match change {
            0 => {
                state.subscriptions().lock().unwrap()[0].url =
                    "https://other.invalid/?token=changed".into()
            }
            1 => {
                state.proxy_configs().lock().unwrap()[0].name =
                    "same-ms different material owner view".into()
            }
            _ => {
                state
                    .app_config()
                    .lock()
                    .unwrap()
                    .mobile_notifications_enabled = true
            }
        }
        assert!(publish_at(
            &state,
            dir.path(),
            &dir.path().join("app-config.json"),
            record.clone(),
            identity(&record).unwrap(),
            Some(record.barrier_source_updated_at_unix_ms)
        )
        .is_err());
        assert_eq!(
            ledger::load(dir.path(), &record.recovery_id)
                .unwrap()
                .unwrap()
                .stage,
            Stage::RuntimeConfirmed
        );
    }
}
#[test]
fn different_instance_or_revision_never_resolves_even_when_local_candidate_matches() {
    for different_instance in [false, true] {
        let (dir, state, record) = fixture();
        let mut observed = identity(&record).unwrap();
        if different_instance {
            observed.core_instance_id = "replacement".into();
        } else {
            observed.config_revision += 1;
        }
        assert!(publish_at(
            &state,
            dir.path(),
            &dir.path().join("app-config.json"),
            record.clone(),
            observed,
            None
        )
        .is_err());
        assert_eq!(
            ledger::load(dir.path(), &record.recovery_id)
                .unwrap()
                .unwrap()
                .stage,
            Stage::RuntimeConfirmed
        );
    }
}

#[test]
fn unrelated_later_legacy_unknown_never_borrows_completed_receipt_or_blocks_inspection() {
    let (dir, state, record) = fixture();
    publish_at(
        &state,
        dir.path(),
        &dir.path().join("app-config.json"),
        record.clone(),
        identity(&record).unwrap(),
        None,
    )
    .unwrap();
    let expected = {
        let mut subscriptions = state.subscriptions().lock().unwrap();
        subscriptions[0].last_error =
            Some(crate::services::subscription_reconciliation::UNKNOWN_APPLY_MESSAGE.into());
        subscriptions[0].updated_at_unix_ms += 1;
        subscriptions[0].updated_at_unix_ms
    };
    super::super::app_database::save_domain_data(
        dir.path(),
        &state.proxy_configs().lock().unwrap(),
        &state.subscriptions().lock().unwrap(),
    )
    .unwrap();
    assert!(inspect_at(&state, dir.path(), "source", expected)
        .unwrap()
        .is_none());
}

#[test]
fn confirmed_local_db_commit_can_promote_only_its_owned_previous_memory_phase() {
    let (dir, state, record) = fixture();
    let local = ledger::publish_local(dir.path(), &record.recovery_id, record.version).unwrap();
    assert_ne!(
        ledger::fingerprint(&*state.proxy_configs().lock().unwrap()).unwrap(),
        ledger::fingerprint(
            &super::super::app_database::load_domain_data(dir.path())
                .unwrap()
                .proxy_configs
        )
        .unwrap()
    );
    let updated = publish_at(
        &state,
        dir.path(),
        &dir.path().join("app-config.json"),
        local,
        identity(&record).unwrap(),
        Some(record.barrier_source_updated_at_unix_ms),
    )
    .unwrap();
    assert!(updated.last_error.is_none());
    assert_eq!(
        state.proxy_configs().lock().unwrap()[0].name,
        "confirmed candidate"
    );
}

#[test]
fn typed_rule_set_changes_after_receipt_invalidate_local_recovery() {
    let (dir, state, record) = fixture();
    let rule: crate::models::rule_set::RuleSetProfile = serde_json::from_value(json!({
        "id":"changed", "name":"Changed", "source":null,
        "enabled":true, "updatedAtUnixMs":1,
        "semanticIr":{"version":1,"name":"Changed","rules":[]},
    }))
    .unwrap();
    state.rule_sets().lock().unwrap().push(rule);
    assert!(publish_at(
        &state,
        dir.path(),
        &dir.path().join("app-config.json"),
        record.clone(),
        identity(&record).unwrap(),
        Some(record.barrier_source_updated_at_unix_ms)
    )
    .is_err());
    assert_eq!(
        ledger::load(dir.path(), &record.recovery_id)
            .unwrap()
            .unwrap()
            .stage,
        Stage::RuntimeConfirmed
    );
}

#[test]
fn complete_commit_delivery_failure_never_rewrites_old_unknown_memory_over_final_source() {
    let (dir, state, record) = fixture();
    let error = finish_confirmed_at(
        &state,
        dir.path(),
        &dir.path().join("app-config.json"),
        record.clone(),
        || Err(AppError::internal("injected post-complete reload failure")),
    )
    .unwrap_err();
    assert_eq!(error.code, "subscription_local_recovery_pending");
    assert_eq!(
        ledger::load(dir.path(), &record.recovery_id)
            .unwrap()
            .unwrap()
            .stage,
        Stage::Complete
    );
    assert!(state.subscriptions().lock().unwrap()[0]
        .last_error
        .is_some());
    assert!(
        !crate::services::subscription::should_publish_sync_error(&state, "source", &error)
            .unwrap()
    );
    let durable = super::super::app_database::load_domain_data(dir.path()).unwrap();
    assert!(durable.subscriptions[0].last_error.is_none());
    assert_eq!(durable.subscriptions[0].last_sync_at_unix_ms, Some(2));
    let complete = ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap();
    let source = publish_at(
        &state,
        dir.path(),
        &dir.path().join("app-config.json"),
        complete,
        identity(&record).unwrap(),
        Some(record.barrier_source_updated_at_unix_ms),
    )
    .unwrap();
    assert!(source.last_error.is_none());
    assert_eq!(source.last_sync_at_unix_ms, Some(2));
}

#[test]
fn original_app_and_rules_history_reject_real_a_b_a_writes_after_reopen() {
    for app_domain in [true, false] {
        let (dir, state, record) = fixture();
        if app_domain {
            let path = dir.path().join("app-config.json");
            let original = state.app_config().lock().unwrap().clone();
            let mut changed = original.clone();
            changed.mobile_notifications_enabled = !changed.mobile_notifications_enabled;
            super::super::app_config_store::save(&path, &changed).unwrap();
            super::super::app_config_store::save(&path, &original).unwrap();
        } else {
            let changed: RuleSetProfile = serde_json::from_value(json!({
                "id":"changed", "name":"Changed", "enabled":true,
                "semanticIr":{"version":1,"name":"Changed","rules":[]}, "updatedAtUnixMs":1
            }))
            .unwrap();
            super::super::domain_store::save_rule_sets_to_dir(dir.path(), &[changed]).unwrap();
            super::super::domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
        }
        let reopened = reopen_state(dir.path(), &state);
        assert!(publish_at(
            &reopened,
            dir.path(),
            &dir.path().join("app-config.json"),
            record.clone(),
            identity(&record).unwrap(),
            None
        )
        .is_err());
        assert_eq!(
            ledger::load(dir.path(), &record.recovery_id)
                .unwrap()
                .unwrap()
                .stage,
            Stage::RuntimeConfirmed
        );
    }
}

#[test]
fn crash_after_owned_file_write_uses_original_owner_without_rewriting_primary() {
    let (dir, state, record) = fixture();
    let local = ledger::publish_local(dir.path(), &record.recovery_id, record.version).unwrap();
    let path = dir.path().join("app-config.json");
    let candidate = projected_app(
        &state.app_config().lock().unwrap(),
        &state.proxy_configs().lock().unwrap(),
    );
    let owned =
        super::super::app_config_store::save_owned(&path, &candidate, &record.recovery_id).unwrap();
    assert_eq!(owned.before, record.expected_app_history.clone().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let history_path = dir.path().join("app-config.json.history.json");
    let history_before = std::fs::read(&history_path).unwrap();
    let reopened = reopen_state(dir.path(), &state);
    assert!(inspect_at(
        &reopened,
        dir.path(),
        "source",
        record.barrier_source_updated_at_unix_ms
    )
    .unwrap()
    .is_some());
    assert_eq!(
        std::fs::read(&history_path).unwrap(),
        history_before,
        "inspection must not checkpoint or confirm metadata"
    );
    assert!(ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap()
        .app_projection_receipt
        .is_none());
    let source = publish_at(
        &reopened,
        dir.path(),
        &path,
        local,
        identity(&record).unwrap(),
        None,
    )
    .unwrap();
    assert!(source.last_error.is_none());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(
        ledger::load(dir.path(), &record.recovery_id)
            .unwrap()
            .unwrap()
            .app_projection_receipt,
        Some(owned)
    );
}

#[test]
fn equal_projection_written_by_another_owner_cannot_be_adopted() {
    let (dir, state, record) = fixture();
    let local = ledger::publish_local(dir.path(), &record.recovery_id, record.version).unwrap();
    let path = dir.path().join("app-config.json");
    let candidate = projected_app(
        &state.app_config().lock().unwrap(),
        &state.proxy_configs().lock().unwrap(),
    );
    super::super::app_config_store::save_owned(&path, &candidate, "another-operation").unwrap();
    assert!(inspect_at(
        &state,
        dir.path(),
        "source",
        record.barrier_source_updated_at_unix_ms
    )
    .is_err());
    assert!(publish_at(
        &state,
        dir.path(),
        &path,
        local,
        identity(&record).unwrap(),
        None
    )
    .is_err());
    assert!(ledger::load(dir.path(), &record.recovery_id)
        .unwrap()
        .unwrap()
        .app_projection_receipt
        .is_none());
}

#[test]
fn pending_owned_history_is_read_only_on_inspect_and_confirmed_only_on_recovery() {
    let (dir, state, record) = fixture();
    let local = ledger::publish_local(dir.path(), &record.recovery_id, record.version).unwrap();
    let path = dir.path().join("app-config.json");
    let candidate = projected_app(
        &state.app_config().lock().unwrap(),
        &state.proxy_configs().lock().unwrap(),
    );
    let mut guard = file_history::acquire(&path, Domain::AppConfig).unwrap();
    let bytes = serde_json::to_vec_pretty(&candidate).unwrap();
    // Simulates a real crash after both writes and before clean history commit.
    // mutate returns an injected failure after the expected primary is durable.
    assert!(guard
        .mutate(&bytes, None, Some(&record.recovery_id), || {
            super::super::atomic_file::write(&path, &bytes).unwrap();
            Err::<(), _>(AppError::internal(
                "injected crash before history acknowledgement",
            ))
        })
        .is_err());
    assert!(guard.is_pending());
    drop(guard);
    let history_path = dir.path().join("app-config.json.history.json");
    let history_bytes = std::fs::read(&history_path).unwrap();
    assert!(inspect_at(
        &state,
        dir.path(),
        "source",
        record.barrier_source_updated_at_unix_ms
    )
    .unwrap()
    .is_some());
    assert_eq!(std::fs::read(&history_path).unwrap(), history_bytes);
    assert!(file_history::acquire(&path, Domain::AppConfig)
        .unwrap()
        .is_pending());
    let result = publish_at(
        &state,
        dir.path(),
        &path,
        local,
        identity(&record).unwrap(),
        None,
    )
    .unwrap();
    assert!(result.last_error.is_none());
    assert!(!file_history::acquire(&path, Domain::AppConfig)
        .unwrap()
        .is_pending());
}

#[test]
fn fresh_os_guard_serializes_a_real_competing_store_writer() {
    let (dir, state, record) = fixture();
    let path = dir.path().join("app-config.json");
    let app = state.app_config().lock().unwrap().clone();
    let writer_path = path.clone();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let mut writer = None;
    with_locked(&state, dir.path(), &path, |locked| {
        locked.evaluate(&record, record.barrier_source_updated_at_unix_ms)?;
        writer = Some(std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let result = super::super::app_config_store::save(&writer_path, &app);
            done_tx.send(result.is_ok()).unwrap();
        }));
        started_rx.recv().unwrap();
        assert!(matches!(
            done_rx.recv_timeout(std::time::Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        locked.evaluate(&record, record.barrier_source_updated_at_unix_ms)?;
        Ok(())
    })
    .unwrap();
    assert!(done_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap());
    writer.unwrap().join().unwrap();
    // Equal bytes written by another owner still consume history. A later
    // recovery cannot adopt them just because the typed settings match.
    assert!(publish_at(
        &state,
        dir.path(),
        &path,
        record.clone(),
        identity(&record).unwrap(),
        None
    )
    .is_err());
}

#[test]
fn deleted_or_corrupt_history_never_rebinds_an_existing_record() {
    for corrupt in [false, true] {
        let (dir, state, record) = fixture();
        let history = dir.path().join("app-config.json.history.json");
        if corrupt {
            std::fs::write(&history, b"{corrupt-private-history").unwrap();
        } else {
            std::fs::remove_file(&history).unwrap();
        }
        assert!(publish_at(
            &state,
            dir.path(),
            &dir.path().join("app-config.json"),
            record.clone(),
            identity(&record).unwrap(),
            None
        )
        .is_err());
        assert_eq!(
            ledger::load(dir.path(), &record.recovery_id)
                .unwrap()
                .unwrap()
                .stage,
            Stage::RuntimeConfirmed
        );
    }
}
