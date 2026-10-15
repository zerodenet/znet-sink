use super::*;
use crate::services::endpoint_restoration::tests::{record, source};
use std::{
    fs,
    sync::{Arc, Barrier},
};
fn fixture() -> (tempfile::TempDir, AppState, AppConfig, Record) {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::default();
    let mut app = AppConfig::default();
    let mut record = record();
    app.profile_edits
        .entry("p".into())
        .or_default()
        .insert("endpoints".into(), json!({"a":record.overlay.clone()}));
    app_config_store::save(&app_path(directory.path()), &app).unwrap();
    app = app_config_store::load_or_default(&app_path(directory.path())).unwrap();
    *state.app_config().lock().unwrap() = app.clone();
    domain_store::save_rule_sets_to_dir(directory.path(), &[]).unwrap();
    record.binding.files = Some(capture_at(&state, &app, directory.path()).unwrap());
    record.plan.client_scope = state.client_core_snapshot().scope;
    record.owner = Some(owner(&state).unwrap());
    record.app_digest = digest(&app).unwrap();
    record.binding.source_digest = digest(&source()).unwrap();
    record.local_write_started = true;
    record.receipt.runtime_confirmed = true;
    record.receipt.apply_identity = Some(RuntimeReceipt {
        core_instance_id: "core-a".into(),
        config_revision: "5".into(),
    });
    (directory, state, app, record)
}
fn next(app: &AppConfig, record: &Record) -> AppConfig {
    endpoints::restore_candidate(app, "p", &source(), &record.binding.snapshot).unwrap()
}
fn rules_b() -> Vec<crate::models::rule_set::RuleSetProfile> {
    vec![serde_json::from_value(
        json!({"id":"other","name":"other","enabled":false,"semanticIr":{},"updatedAtUnixMs":1}),
    )
    .unwrap()]
}
#[test]
fn persisted_app_primary_and_backup_aba_reject_old_ticket_even_when_both_bytes_return() {
    let (directory, state, app, mut record) = fixture();
    let path = app_path(directory.path());
    // The loader normalizes default bypass settings. Establish both actual
    // files at that canonical A before issuing this test's original ticket.
    app_config_store::save(&path, &app).unwrap();
    app_config_store::save(&path, &app).unwrap();
    record.binding.files = Some(capture_at(&state, &app, directory.path()).unwrap());
    let original_primary = fs::read(&path).unwrap();
    let original_backup = fs::read(path.with_extension("json.bak")).unwrap();
    let encoded = serde_json::to_vec(&record).unwrap();
    let old: Record = serde_json::from_slice(&encoded).unwrap();
    let mut changed = app.clone();
    changed.core.auto_start = !changed.core.auto_start;
    app_config_store::save(&path, &changed).unwrap();
    app_config_store::save(&path, &app).unwrap();
    app_config_store::save(&path, &app).unwrap();
    assert_eq!(fs::read(&path).unwrap(), original_primary);
    assert_eq!(
        fs::read(path.with_extension("json.bak")).unwrap(),
        original_backup
    );
    assert!(require_at(&state, files(&old).unwrap(), &app, directory.path()).is_err());
    assert!(publish_at(&state, &old, &app, next(&app, &old), directory.path()).is_err());
    assert_eq!(*state.app_config().lock().unwrap(), app);
}
#[test]
fn official_rules_aba_and_same_value_writes_advance_history_and_reject_prepared_material() {
    let (directory, state, app, record) = fixture();
    let original = fs::read(rule_path(directory.path())).unwrap();
    domain_store::save_rule_sets_to_dir(directory.path(), &rules_b()).unwrap();
    domain_store::save_rule_sets_to_dir(directory.path(), &[]).unwrap();
    assert_eq!(fs::read(rule_path(directory.path())).unwrap(), original);
    assert!(require_at(&state, files(&record).unwrap(), &app, directory.path()).is_err());
    assert!(publish_at(&state, &record, &app, next(&app, &record), directory.path()).is_err());
}
#[test]
fn exact_owned_publication_returns_receipt_and_read_only_confirmation_never_rewrites_bytes() {
    let (directory, state, app, mut record) = fixture();
    let candidate = next(&app, &record);
    let receipt = publish_at(&state, &record, &app, candidate.clone(), directory.path()).unwrap();
    assert_eq!(receipt.before, files(&record).unwrap().app);
    assert_eq!(
        receipt.owner_operation.as_deref(),
        Some(record.plan.plan_id.as_str())
    );
    record.app_write = Some(receipt.clone());
    let path = app_path(directory.path());
    let primary = fs::read(&path).unwrap();
    let backup = fs::read(path.with_extension("json.bak")).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(
        confirm_local_at(&state, &record, &source(), directory.path()).unwrap(),
        receipt
    );
    assert_eq!(
        confirm_local_at(&state, &record, &source(), directory.path()).unwrap(),
        receipt
    );
    assert_eq!(*state.app_config().lock().unwrap(), candidate);
    assert_eq!(fs::read(&path).unwrap(), primary);
    assert_eq!(fs::read(path.with_extension("json.bak")).unwrap(), backup);
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
}
#[test]
fn original_lost_file_receipt_confirms_only_owned_metadata_after_actual_partial_outcome() {
    let (directory, state, app, record) = fixture();
    let candidate = next(&app, &record);
    let path = app_path(directory.path());
    let bytes = serde_json::to_vec_pretty(&candidate).unwrap();
    let backup = serde_json::to_vec_pretty(&app).unwrap();
    let mut guard = file_history::acquire(&path, Domain::AppConfig).unwrap();
    guard.require(&files(&record).unwrap().app).unwrap();
    let result: AppResult<((), WriteReceipt)> =
        guard.mutate(&bytes, Some(&backup), Some(&record.plan.plan_id), || {
            crate::services::atomic_file::write(&path.with_extension("json.bak"), &backup).unwrap();
            crate::services::atomic_file::write(&path, &bytes).unwrap();
            Err(AppError::internal("test fixture lost the write receipt"))
        });
    assert!(result.is_err());
    assert!(guard.is_pending());
    drop(guard);
    let primary = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let restored: Record = serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
    let mut tampered = restored.clone();
    let before = files(&restored).unwrap().app.clone();
    tampered.app_write = Some(WriteReceipt {
        before: before.clone(),
        after: before,
        owner_operation: Some(restored.plan.plan_id.clone()),
    });
    let pending_metadata = fs::read(path.with_extension("json.history.json")).unwrap();
    assert!(confirm_local_at(&state, &tampered, &source(), directory.path()).is_err());
    assert_eq!(
        fs::read(path.with_extension("json.history.json")).unwrap(),
        pending_metadata
    );
    let receipt = confirm_local_at(&state, &restored, &source(), directory.path()).unwrap();
    assert_eq!(receipt.before, files(&record).unwrap().app);
    assert_eq!(
        receipt.owner_operation.as_deref(),
        Some(record.plan.plan_id.as_str())
    );
    assert!(!file_history::acquire(&path, Domain::AppConfig)
        .unwrap()
        .is_pending());
    assert_eq!(fs::read(&path).unwrap(), primary);
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
}
#[test]
fn another_owner_or_later_identical_write_cannot_supply_original_operation_receipt() {
    for mode in ["other-owner", "later-same-value"] {
        let (directory, state, app, mut record) = fixture();
        let candidate = next(&app, &record);
        let path = app_path(directory.path());
        if mode == "other-owner" {
            app_config_store::save_owned(&path, &candidate, "unrelated-operation").unwrap();
        } else {
            record.app_write = Some(
                publish_at(&state, &record, &app, candidate.clone(), directory.path()).unwrap(),
            );
            app_config_store::save(&path, &candidate).unwrap();
        }
        let primary = fs::read(&path).unwrap();
        let metadata = fs::read(path.with_extension("json.history.json")).unwrap();
        assert!(confirm_local_at(&state, &record, &source(), directory.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), primary);
        assert_eq!(
            fs::read(path.with_extension("json.history.json")).unwrap(),
            metadata
        );
    }
}
#[test]
fn rules_change_after_owned_write_prevents_metadata_confirmation_without_replaying_local_write() {
    let (directory, state, app, mut record) = fixture();
    record.app_write =
        Some(publish_at(&state, &record, &app, next(&app, &record), directory.path()).unwrap());
    domain_store::save_rule_sets_to_dir(directory.path(), &rules_b()).unwrap();
    domain_store::save_rule_sets_to_dir(directory.path(), &[]).unwrap();
    let before = fs::read(app_path(directory.path())).unwrap();
    assert!(confirm_local_at(&state, &record, &source(), directory.path()).is_err());
    assert_eq!(fs::read(app_path(directory.path())).unwrap(), before);
}
#[test]
fn concurrent_same_token_local_publishers_commit_once_and_preserve_original_owned_receipt() {
    let (directory, state, app, record) = fixture();
    let state = Arc::new(state);
    let barrier = Arc::new(Barrier::new(2));
    let workers = (0..2)
        .map(|_| {
            let state = state.clone();
            let barrier = barrier.clone();
            let app = app.clone();
            let record = record.clone();
            let dir = directory.path().to_owned();
            std::thread::spawn(move || {
                barrier.wait();
                publish_at(&state, &record, &app, next(&app, &record), &dir).is_ok()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|ok| *ok)
            .count(),
        1
    );
    let after = app_config_store::current_history(&app_path(directory.path())).unwrap();
    assert_eq!(after.epoch, files(&record).unwrap().app.epoch + 1);
}
#[test]
fn restored_old_host_and_client_aba_lose_prepared_authority_despite_equal_current_content() {
    let (directory, state, app, record) = fixture();
    let old: Record = serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
    let replacement = AppState::default();
    *replacement.app_config().lock().unwrap() = app.clone();
    assert_eq!(
        state.client_core_snapshot().scope_generation,
        replacement.client_core_snapshot().scope_generation
    );
    assert!(require_owner(&replacement, &old).is_err());
    assert!(publish_at(&replacement, &old, &app, next(&app, &old), directory.path()).is_err());
    state.client_core_configuration_committed(None);
    assert!(require_owner(&state, &old).is_err());
    assert!(publish_at(&state, &old, &app, next(&app, &old), directory.path()).is_err());
}
#[test]
fn legacy_records_and_missing_original_local_intent_remain_read_only() {
    let (directory, state, app, record) = fixture();
    let mut value = serde_json::to_value(&record).unwrap();
    for name in ["owner", "local_write_started", "save_only", "app_write"] {
        value.as_object_mut().unwrap().remove(name);
    }
    value["binding"].as_object_mut().unwrap().remove("files");
    let legacy: Record = serde_json::from_value(value).unwrap();
    assert!(require_owner(&state, &legacy).is_err());
    assert!(publish_at(&state, &legacy, &app, next(&app, &legacy), directory.path()).is_err());
    assert!(confirm_local_at(&state, &legacy, &source(), directory.path()).is_err());
    let mut prepared = record.clone();
    prepared.local_write_started = false;
    assert!(publish_at(
        &state,
        &prepared,
        &app,
        next(&app, &prepared),
        directory.path()
    )
    .is_err());
}
#[cfg(unix)]
#[test]
fn actual_local_io_failure_does_not_publish_or_turn_a_readable_old_value_into_success() {
    use std::os::unix::fs::PermissionsExt;
    let (directory, state, app, record) = fixture();
    let path = app_path(directory.path());
    let before = fs::read(&path).unwrap();
    if unsafe { libc::geteuid() } != 0 {
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = publish_at(&state, &record, &app, next(&app, &record), directory.path());
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
    } else {
        let history = path.with_extension("json.history.json");
        fs::remove_file(&history).unwrap();
        fs::create_dir(&history).unwrap();
        assert!(publish_at(&state, &record, &app, next(&app, &record), directory.path()).is_err());
    }
    assert_eq!(*state.app_config().lock().unwrap(), app);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(confirm_local_at(&state, &record, &source(), directory.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
