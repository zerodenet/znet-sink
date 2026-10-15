use super::*;
use crate::configuration::workspace;
fn candidate(state: &AppState) -> Prepared {
    let mut p = workspace::tests::prepared(state);
    p.source["runtime"]["udp_upstream_idle_timeout_seconds"] = json!(123);
    let (_, source, effective) = workspace::from_input(
        state,
        &ConfigWorkspaceDraftInput {
            binding: local_snapshot(state).unwrap().binding.unwrap(),
            source_json: p.source.to_string(),
        },
    )
    .unwrap();
    p.source = source;
    p.effective = effective;
    p.plan.source_json = pretty(&p.source).unwrap();
    p.plan.effective_json = pretty(&p.effective).unwrap();
    p.plan.effective_digest = digest(&p.effective).unwrap();
    p
}
fn submitted(state: &AppState) -> String {
    let p = store(state, candidate(state)).unwrap();
    submit(state, &p.plan_id).unwrap();
    p.plan_id
}
fn confirmed(state: &AppState, id: &str) {
    let meta = metadata(state, id).unwrap().unwrap();
    workspace::runtime_confirmed(
        state,
        id,
        ConfigTransactionReceipt {
            transaction_id: u64::MAX,
            profile_id: meta.profile_id,
            strategy: ConfigApplyStrategy::HotReload,
            state: "runtime_confirmed",
            effective_digest: meta.effective_digest,
            started_at_unix_ms: meta.created_at_unix_ms,
            completed_at_unix_ms: meta.created_at_unix_ms,
            runtime_identity: ConfigRuntimeIdentityView {
                core_instance_id: "core".into(),
                config_revision: u64::MAX,
            },
        },
    )
    .unwrap();
}
fn reopen(state: &AppState) -> AppState {
    let dir = directory(state).unwrap();
    let graph = app_database::load_domain_data(&dir).unwrap();
    let next = AppState::with_domain_data(
        app_config_store::load_or_default(&dir.join("app-config.json")).unwrap(),
        graph.proxy_configs,
        graph.subscriptions,
        state.rule_sets().lock().unwrap().clone(),
        vec![],
    );
    *next.configuration().plans.1.lock().unwrap() = Some(dir);
    next
}
#[test]
fn durable_unknown_survives_new_owner_without_reviving_original_plan_or_guessing_receipt() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    let next = reopen(&state);
    assert_ne!(
        state.host_session_id().unwrap(),
        next.host_session_id().unwrap()
    );
    let op = operation(&next, &id).unwrap();
    assert_eq!(op.state, "submitted_unknown");
    assert!(op.receipt.is_none());
    assert!(ensure_unblocked(&next, "ordinary").is_err());
    assert!(review(&next, &id).is_err());
    assert!(submit(&next, &id).is_err());
    let snap = local_snapshot(&next).unwrap();
    assert_eq!(snap.pending_operation.unwrap().plan_id, id);
    assert!(!snap.runtime.confirmed);
}
#[test]
fn original_confirmed_receipt_completes_only_local_candidate_after_reopen_and_repeat_is_read_only()
{
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let next = reopen(&state);
    let record = load_checked(&next, &id).unwrap();
    assert_eq!(record.meta.stage, Stage::RuntimeConfirmedPendingLocal);
    let before_app = next.app_config().lock().unwrap().clone();
    let binding = context(&next).unwrap().binding;
    let result = finish_local(&next, &id, &binding, record.meta.version).unwrap();
    assert_eq!(result.state, "confirmed");
    assert!(pending_operation(&next, "ordinary").unwrap().is_none());
    assert!(ensure_unblocked(&next, "ordinary").is_ok());
    assert!(store(&next, candidate(&next)).is_ok());
    assert_eq!(result.receipt.unwrap().transaction_id, u64::MAX);
    let done = db::load(&directory(&next).unwrap(), &id).unwrap().unwrap();
    assert_eq!(done.meta.stage, Stage::Complete);
    assert!(done.candidate.is_none());
    let content = next.proxy_configs().lock().unwrap()[0]
        .content
        .clone()
        .unwrap();
    assert_eq!(content["runtime"]["udp_upstream_idle_timeout_seconds"], 123);
    assert_eq!(*next.app_config().lock().unwrap(), before_app);
    let timestamp = next.proxy_configs().lock().unwrap()[0].updated_at_unix_ms;
    let epoch = db::current_epoch(&directory(&next).unwrap()).unwrap();
    let again = reopen(&next);
    let done = load_checked(&again, &id).unwrap();
    let binding = context(&again).unwrap().binding;
    finish_local(&again, &id, &binding, done.meta.version).unwrap();
    assert_eq!(
        again.proxy_configs().lock().unwrap()[0].updated_at_unix_ms,
        timestamp
    );
    assert_eq!(
        db::current_epoch(&directory(&again).unwrap()).unwrap(),
        epoch
    );
    assert_eq!(
        metadata(&again, &id).unwrap().unwrap().version,
        done.meta.version
    );
}
#[test]
fn sqlite_commit_then_app_file_failure_retains_receipt_and_resumes_local_stages_without_reapply() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let dir = directory(&state).unwrap();
    let record = load_checked(&state, &id).unwrap();
    let binding = context(&state).unwrap().binding;
    let file = dir.join("app-config.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(finish_local(&state, &id, &binding, record.meta.version).is_err());
    let meta = metadata(&state, &id).unwrap().unwrap();
    assert_eq!(meta.stage, Stage::LocalCommitted);
    assert!(meta.receipt.is_some());
    assert!(ensure_unblocked(&state, "ordinary").is_err());
    std::fs::remove_dir(&file).unwrap();
    app_config_store::save(&file, &state.app_config().lock().unwrap()).unwrap();
    let next = reopen(&state);
    let r = load_checked(&next, &id).unwrap();
    let epoch = r.meta.epoch;
    let binding = context(&next).unwrap().binding;
    finish_local(&next, &id, &binding, r.meta.version).unwrap();
    assert_eq!(metadata(&next, &id).unwrap().unwrap().epoch, epoch);
    assert_eq!(
        metadata(&next, &id).unwrap().unwrap().stage,
        Stage::Complete
    );
}
#[test]
fn same_timestamp_relation_aba_is_rejected_by_persistent_epoch_across_owner_restart() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let dir = directory(&state).unwrap();
    let profiles = state.proxy_configs().lock().unwrap().clone();
    let mut changed = profiles.clone();
    changed[0].name = "B".into();
    app_database::save_domain_data(&dir, &changed, &[]).unwrap();
    app_database::save_domain_data(&dir, &profiles, &[]).unwrap();
    let next = reopen(&state);
    assert!(load_checked(&next, &id).is_err());
    assert!(ensure_unblocked(&next, "ordinary").is_err());
    assert_eq!(next.proxy_configs().lock().unwrap()[0].name, "ordinary");
}
#[test]
fn changed_app_rules_or_generation_never_publish_original_candidate() {
    for mutation in ["app", "rules", "generation"] {
        let state = workspace::tests::state();
        let id = submitted(&state);
        confirmed(&state, &id);
        let r = load_checked(&state, &id).unwrap();
        let binding = context(&state).unwrap().binding;
        if mutation == "app" {
            state
                .app_config()
                .lock()
                .unwrap()
                .routing
                .inject_common_rules = true;
        } else if mutation == "rules" {
            state.rule_sets().lock().unwrap().push(serde_json::from_value(json!({"id":"r","name":"r","enabled":false,"semanticIr":{"version":1,"rules":[]},"updatedAtUnixMs":1})).unwrap());
        } else {
            let p = state.proxy_configs().lock().unwrap()[0].clone();
            state.client_core_configuration_committed(Some(&p));
        }
        assert!(
            finish_local(&state, &id, &binding, r.meta.version).is_err(),
            "{mutation}"
        );
        assert_eq!(
            metadata(&state, &id).unwrap().unwrap().stage,
            Stage::RuntimeConfirmedPendingLocal
        );
        assert_eq!(
            state.proxy_configs().lock().unwrap()[0]
                .content
                .as_ref()
                .unwrap()["runtime"]["udp_upstream_idle_timeout_seconds"],
            json!(u64::MAX)
        );
    }
}
#[test]
fn review_nonce_is_one_use_and_cannot_survive_new_owner_or_configuration_aba() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let r = load_checked(&state, &id).unwrap();
    let (nonce, _) = issue_review(&state, &r).unwrap();
    let next = reopen(&state);
    assert!(consume_review(&next, &id, &nonce).is_err());
    let p = state.proxy_configs().lock().unwrap()[0].clone();
    state.client_core_configuration_committed(Some(&p));
    assert!(consume_review(&state, &id, &nonce).is_err());
    let (nonce, _) = issue_review(&state, &r).unwrap();
    consume_review(&state, &id, &nonce).unwrap();
    assert!(consume_review(&state, &id, &nonce).is_err());
}

#[test]
fn child_reopen_driver() {
    let Some(dir) = std::env::var_os("ZNET_WORKSPACE_RECOVERY_CHILD_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let id = std::env::var("ZNET_WORKSPACE_RECOVERY_CHILD_ID").unwrap();
    let mode = std::env::var("ZNET_WORKSPACE_RECOVERY_CHILD_MODE").unwrap();
    let data = app_database::load_domain_data(&dir).unwrap();
    let state = AppState::with_domain_data(
        app_config_store::load_or_default(&dir.join("app-config.json")).unwrap(),
        data.proxy_configs,
        data.subscriptions,
        vec![],
        vec![],
    );
    *state.configuration().plans.1.lock().unwrap() = Some(dir.clone());
    let origin = metadata(&state, &id).unwrap().unwrap();
    assert_ne!(origin.session, state.host_session_id().unwrap());
    if mode == "unknown" {
        assert_eq!(operation(&state, &id).unwrap().state, "submitted_unknown");
        assert!(operation(&state, &id).unwrap().receipt.is_none());
        assert!(ensure_unblocked(&state, "ordinary").is_err());
        assert!(submit(&state, &id).is_err());
    } else {
        let r = load_checked(&state, &id).unwrap();
        let binding = context(&state).unwrap().binding;
        let op = finish_local(&state, &id, &binding, r.meta.version).unwrap();
        assert_eq!(op.state, "confirmed");
        assert_eq!(
            state.proxy_configs().lock().unwrap()[0]
                .content
                .as_ref()
                .unwrap()["runtime"]["udp_upstream_idle_timeout_seconds"],
            123
        );
    }
}
#[test]
fn separate_os_process_reopens_unknown_barrier_and_original_confirmed_local_material() {
    for mode in ["unknown", "confirmed"] {
        let state = workspace::tests::state();
        let id = submitted(&state);
        if mode == "confirmed" {
            confirmed(&state, &id);
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "configuration::workspace::recovery::tests::child_reopen_driver",
                "--nocapture",
            ])
            .env(
                "ZNET_WORKSPACE_RECOVERY_CHILD_DIR",
                directory(&state).unwrap(),
            )
            .env("ZNET_WORKSPACE_RECOVERY_CHILD_ID", &id)
            .env("ZNET_WORKSPACE_RECOVERY_CHILD_MODE", mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        assert_eq!(
            metadata(&state, &id).unwrap().unwrap().stage,
            if mode == "unknown" {
                Stage::SubmittedUnknown
            } else {
                Stage::Complete
            }
        );
    }
}
#[test]
fn malformed_metadata_or_redundant_columns_fail_closed_without_reading_candidate_or_hiding_barrier()
{
    for mutation in [
        "column-stage",
        "column-profile",
        "column-version",
        "bad-session",
        "bad-digest",
        "receipt-on-unknown",
    ] {
        let state = workspace::tests::state();
        let id = submitted(&state);
        let dir = directory(&state).unwrap();
        let conn = rusqlite::Connection::open(dir.join("znet-sink.db")).unwrap();
        match mutation {
            "column-stage" => {
                conn.execute(
                    "UPDATE workspace_recovery SET stage='complete' WHERE operation_id=?1",
                    [&id],
                )
                .unwrap();
            }
            "column-profile" => {
                conn.execute(
                    "UPDATE workspace_recovery SET profile_id='other' WHERE operation_id=?1",
                    [&id],
                )
                .unwrap();
            }
            "column-version" => {
                conn.execute(
                    "UPDATE workspace_recovery SET version=version+1 WHERE operation_id=?1",
                    [&id],
                )
                .unwrap();
            }
            _ => {
                let raw: String = conn
                    .query_row(
                        "SELECT metadata_json FROM workspace_recovery WHERE operation_id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .unwrap();
                let mut v: Value = serde_json::from_str(&raw).unwrap();
                if mutation == "bad-session" {
                    v["session"] = json!("not-hex-secret-canary");
                } else if mutation == "bad-digest" {
                    v["app_digest"] = json!("secret-canary");
                } else {
                    v["receipt"] = json!({"transaction_id":1,"core_instance_id":"core","config_revision":2,"started_at_unix_ms":1,"completed_at_unix_ms":2});
                }
                conn.execute("UPDATE workspace_recovery SET metadata_json=?2,candidate_json='false' WHERE operation_id=?1",rusqlite::params![id,v.to_string()]).unwrap();
            }
        }
        assert!(metadata(&state, &id).is_err(), "{mutation}");
        let error = pending(&state, "ordinary").err().expect(mutation);
        assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
        assert!(ensure_unblocked(&state, "ordinary").is_err());
    }
}
#[test]
fn sqlite_confirmation_failure_keeps_durable_unknown_even_if_memory_observed_a_real_receipt() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    let dir = directory(&state).unwrap();
    let conn = rusqlite::Connection::open(dir.join("znet-sink.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_receipt BEFORE UPDATE ON workspace_recovery WHEN NEW.stage='runtime_confirmed_pending_local' BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    let meta = metadata(&state, &id).unwrap().unwrap();
    let result = workspace::runtime_confirmed(
        &state,
        &id,
        ConfigTransactionReceipt {
            transaction_id: 1,
            profile_id: "ordinary".into(),
            strategy: ConfigApplyStrategy::HotReload,
            state: "runtime_confirmed",
            effective_digest: meta.effective_digest,
            started_at_unix_ms: meta.created_at_unix_ms,
            completed_at_unix_ms: meta.created_at_unix_ms,
            runtime_identity: ConfigRuntimeIdentityView {
                core_instance_id: "core".into(),
                config_revision: 2,
            },
        },
    );
    assert!(result.is_err());
    let next = reopen(&state);
    assert_eq!(operation(&next, &id).unwrap().state, "submitted_unknown");
    assert!(operation(&next, &id).unwrap().receipt.is_none());
    assert!(ensure_unblocked(&next, "ordinary").is_err());
}

#[test]
fn failed_readonly_projection_leaves_memory_and_generation_then_retry_invalidates_once() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let dir = directory(&state).unwrap();
    let meta = metadata(&state, &id).unwrap().unwrap();
    let r = db::publish(&dir, &id, meta.version).unwrap();
    let generation = state.client_core_snapshot().scope_generation;
    let file = dir.join("znet-sink.db");
    let saved = dir.join("saved.sqlite");
    std::fs::rename(&file, &saved).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(reconcile(&state, &r).is_err());
    assert_eq!(state.client_core_snapshot().scope_generation, generation);
    assert_eq!(
        state.proxy_configs().lock().unwrap()[0]
            .content
            .as_ref()
            .unwrap()["runtime"]["udp_upstream_idle_timeout_seconds"],
        json!(u64::MAX)
    );
    std::fs::remove_dir(&file).unwrap();
    std::fs::rename(saved, file).unwrap();
    reconcile(&state, &r).unwrap();
    let changed = state.client_core_snapshot().scope_generation;
    assert!(changed.0 > generation.0);
    reconcile(&state, &r).unwrap();
    assert_eq!(state.client_core_snapshot().scope_generation, changed);
}
#[test]
fn wall_clock_regression_preserves_original_confirmed_evidence_without_inventing_order() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    let meta = metadata(&state, &id).unwrap().unwrap();
    workspace::runtime_confirmed(
        &state,
        &id,
        ConfigTransactionReceipt {
            transaction_id: 1,
            profile_id: "ordinary".into(),
            strategy: ConfigApplyStrategy::HotReload,
            state: "runtime_confirmed",
            effective_digest: meta.effective_digest,
            started_at_unix_ms: 2,
            completed_at_unix_ms: 1,
            runtime_identity: ConfigRuntimeIdentityView {
                core_instance_id: "core".into(),
                config_revision: 2,
            },
        },
    )
    .unwrap();
    let r = load_checked(&state, &id).unwrap();
    assert!(r.meta.receipt.as_ref().unwrap().clock_regressed);
    assert_eq!(r.meta.stage, Stage::RuntimeConfirmedPendingLocal);
}
#[test]
fn actual_settings_file_drift_is_not_overwritten_by_local_completion() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let dir = directory(&state).unwrap();
    let mut external = state.app_config().lock().unwrap().clone();
    external.routing.inject_common_rules = true;
    app_config_store::save(&dir.join("app-config.json"), &external).unwrap();
    let r = load_checked(&state, &id).unwrap();
    let binding = context(&state).unwrap().binding;
    assert!(finish_local(&state, &id, &binding, r.meta.version).is_err());
    assert_eq!(
        app_config_store::load_or_default(&dir.join("app-config.json")).unwrap(),
        external
    );
    assert_eq!(
        metadata(&state, &id).unwrap().unwrap().stage,
        Stage::RuntimeConfirmedPendingLocal
    );
}

#[test]
fn preexisting_preview_and_local_save_do_not_freshen_persistent_epoch_after_external_aba() {
    let state = workspace::tests::state();
    let p = candidate(&state);
    let input = ConfigWorkspaceDraftInput {
        binding: digest(&p.binding).unwrap(),
        source_json: p.source.to_string(),
    };
    let plan = store(&state, p).unwrap();
    let dir = directory(&state).unwrap();
    let profiles = state.proxy_configs().lock().unwrap().clone();
    let mut b = profiles.clone();
    b[0].name = "B".into();
    app_database::save_domain_data(&dir, &b, &[]).unwrap();
    app_database::save_domain_data(&dir, &profiles, &[]).unwrap();
    assert!(submit(&state, &plan.plan_id).is_err());
    assert!(save(&state, input).is_err());
    assert!(metadata(&state, &plan.plan_id).unwrap().is_none());
}
#[test]
fn recovery_review_expires_by_monotonic_deadline_without_extending_on_wall_clock_changes() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let r = load_checked(&state, &id).unwrap();
    let (nonce, _) = issue_review(&state, &r).unwrap();
    state
        .configuration()
        .plans
        .2
        .lock()
        .unwrap()
        .get_mut(&nonce)
        .unwrap()
        .deadline = std::time::Instant::now() - std::time::Duration::from_secs(1);
    assert!(consume_review(&state, &id, &nonce).is_err());
    assert!(consume_review(&state, &id, &nonce).is_err());
}

#[test]
fn actual_local_save_uses_private_sqlite_epoch_cas_and_preserves_runtime_proxy_owner() {
    let state = workspace::tests::state();
    let dir = directory(&state).unwrap();
    state
        .app_config()
        .lock()
        .unwrap()
        .local_proxy
        .source_proxy_config_id = Some("existing-runtime-owner".into());
    app_config_store::save(
        &dir.join("app-config.json"),
        &state.app_config().lock().unwrap(),
    )
    .unwrap();
    let p = candidate(&state);
    let result = save(
        &state,
        ConfigWorkspaceDraftInput {
            binding: digest(&p.binding).unwrap(),
            source_json: p.source.to_string(),
        },
    )
    .unwrap();
    assert_eq!(result.state, "saved_pending_apply");
    assert!(!result.snapshot.runtime.confirmed);
    assert_eq!(
        state
            .app_config()
            .lock()
            .unwrap()
            .local_proxy
            .source_proxy_config_id
            .as_deref(),
        Some("existing-runtime-owner")
    );
    let reopened = reopen(&state);
    assert_eq!(
        reopened.proxy_configs().lock().unwrap()[0]
            .content
            .as_ref()
            .unwrap()["runtime"]["udp_upstream_idle_timeout_seconds"],
        123
    );
    assert!(reopened.configuration().transaction().is_none());
}
#[test]
fn same_process_confirmed_local_completion_retires_only_original_memory_barrier() {
    let state = workspace::tests::state();
    let id = submitted(&state);
    confirmed(&state, &id);
    let record = load_checked(&state, &id).unwrap();
    let binding = context(&state).unwrap().binding;
    finish_local(&state, &id, &binding, record.meta.version).unwrap();
    assert!(pending_operation(&state, "ordinary").unwrap().is_none());
    assert!(ensure_unblocked(&state, "ordinary").is_ok());
    assert!(store(&state, candidate(&state)).is_ok());
    let again = reopen(&state);
    let snap = local_snapshot(&again).unwrap();
    assert_eq!(snap.last_transaction.unwrap().transaction_id, u64::MAX);
}
