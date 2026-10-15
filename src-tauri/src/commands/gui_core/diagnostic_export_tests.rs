use super::*;
use crate::models::logs::{LogEntry, LogLevel, LogSource};
use serde_json::{json, Value};

#[test]
fn desktop_export_delivers_only_original_v3_public_projection_and_preserves_raw_storage() {
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("core.log");
    let raw_bytes = b"unclassified-canary password free text without a URL";
    fs::write(&raw, raw_bytes).unwrap();
    let state = AppState::default();
    state.logs().lock().unwrap().push(LogEntry {
        id: 1,
        source: LogSource::App,
        level: LogLevel::Error,
        message: format!("unclassified-canary password free text{}", "x".repeat(MAX_BYTES + 1)),
        fields: Some(json!({"status":"arbitrary-canary","id":"opaque-canary","rx_bytes":87654321,"nested":{"unexpected":"unknown-canary"}})),
        occurred_at_unix_ms: 10,
        diagnostic_event: None,
    });
    let preview = diagnostics::preview(&state).unwrap();
    state.logs().lock().unwrap().push(LogEntry {
        id: 2,
        source: LogSource::Core,
        level: LogLevel::Info,
        message: "later-secret-canary".into(),
        fields: None,
        occurred_at_unix_ms: 11,
        diagnostic_event: None,
    });
    let receipt =
        export_with_root(&state, &preview.plan_id, || Ok(root.path().to_owned())).unwrap();
    assert_eq!(receipt.files, [FILE_NAME]);
    let bytes = fs::read(Path::new(&receipt.directory).join(FILE_NAME)).unwrap();
    assert_eq!(bytes.len(), preview.bytes as usize);
    assert_eq!(state.logs().lock().unwrap().len(), 2);
    assert!(state.logs().lock().unwrap()[0]
        .message
        .contains("unclassified-canary"));
    assert!(bytes.len() <= MAX_BYTES);
    let text = String::from_utf8(bytes).unwrap();
    for canary in [
        "unclassified-canary",
        "arbitrary-canary",
        "opaque-canary",
        "unknown-canary",
        "87654321",
        "later-secret-canary",
    ] {
        assert!(
            !text.contains(canary),
            "untrusted material escaped projection"
        );
    }
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["schemaVersion"], "znet.mobile-diagnostics.v3");
    assert_eq!(value["logs"].as_array().unwrap().len(), 1);
    assert_eq!(value["logs"][0]["messageOmitted"], true);
    assert!(value["omissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["category"] == "rawLogFields"));
    assert_eq!(fs::read(&raw).unwrap(), raw_bytes);
    assert!(export_with_root(&state, &preview.plan_id, || panic!(
        "replay reached filesystem"
    ))
    .is_err());
    assert_eq!(fs::read_dir(&receipt.directory).unwrap().count(), 1);
}

#[test]
fn failed_destination_consumes_original_plan_without_raw_fallback_or_raw_error() {
    let root = tempfile::tempdir().unwrap();
    let blocked = root.path().join("private-canary-destination");
    fs::write(&blocked, "not a directory").unwrap();
    let state = AppState::default();
    let preview = diagnostics::preview(&state).unwrap();
    let error = export_with_root(&state, &preview.plan_id, || Ok(blocked.clone()))
        .err()
        .unwrap();
    assert!(!error.message.contains("private-canary"));
    assert!(error.message.contains("未确认"));
    assert!(export_with_root(&state, &preview.plan_id, || panic!("failed plan replayed")).is_err());
    assert_eq!(fs::read_to_string(blocked).unwrap(), "not a directory");
}

#[test]
fn missing_or_changed_scope_plan_never_opens_destination() {
    let state = AppState::default();
    assert!(export_with_root(&state, "absent", || panic!(
        "missing plan opened destination"
    ))
    .is_err());
    let preview = diagnostics::preview(&state).unwrap();
    state.client_core_instance_started();
    assert!(export_with_root(&state, &preview.plan_id, || panic!(
        "old scope opened destination"
    ))
    .is_err());
}

#[test]
fn destination_resolution_failure_is_single_use_and_no_output_is_claimed() {
    let state = AppState::default();
    let preview = diagnostics::preview(&state).unwrap();
    assert!(
        export_with_root(&state, &preview.plan_id, || Err(AppError::internal(
            "no destination"
        )))
        .is_err()
    );
    assert!(diagnostics::take(&state, &preview.plan_id).is_err());
}

#[test]
fn final_output_budget_rejects_oversized_projection_before_creating_directory() {
    let root = tempfile::tempdir().unwrap();
    assert!(write_prepared(root.path(), &vec![0; MAX_BYTES + 1]).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn only_desktop_host_main_window_may_use_local_directory_export() {
    assert!(admit("main", false).is_ok());
    assert!(admit("main", true).is_err());
    assert!(admit("plugin-guest", false).is_err());
}

#[test]
fn discarded_previews_release_shared_capacity_but_cannot_undo_delivery() {
    let state = AppState::default();
    for _ in 0..8 {
        let preview = diagnostics::preview(&state).unwrap();
        diagnostics::discard(&state, &preview.plan_id).unwrap();
        assert!(export_with_root(&state, &preview.plan_id, || panic!(
            "cancelled plan reached IO"
        ))
        .is_err());
    }
    let preview = diagnostics::preview(&state).unwrap();
    diagnostics::take(&state, &preview.plan_id).unwrap();
    diagnostics::discard(&state, &preview.plan_id).unwrap();
    assert!(diagnostics::take(&state, &preview.plan_id).is_err());
}

fn temporary_profile() -> crate::models::proxy_config::ProxyConfigProfile {
    crate::models::proxy_config::ProxyConfigProfile {
        id: "transient-profile".into(),
        name: "test fixture".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(json!({"outbounds":[]})),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: Default::default(),
    }
}

#[test]
fn scope_a_b_a_before_io_refuses_without_writing_or_replaying() {
    let state = AppState::default();
    let original = state.client_core_snapshot();
    let plan = diagnostics::preview(&state).unwrap();
    let root = tempfile::tempdir().unwrap();
    let result = export_with_delivery(
        &state,
        &plan.plan_id,
        || {
            state.client_core_configuration_committed(Some(&temporary_profile()));
            state.client_core_configuration_committed(None);
            assert_eq!(state.client_core_snapshot().scope, original.scope);
            Ok(root.path().to_owned())
        },
        |_, _| panic!("invalidated generation reached IO"),
    );
    assert_eq!(result.err().unwrap().code, "conflict");
    assert!(diagnostics::take(&state, &plan.plan_id).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn scope_a_b_a_during_io_preserves_safe_file_but_returns_unknown_and_cannot_replay() {
    let state = AppState::default();
    let original = state.client_core_snapshot();
    let plan = diagnostics::preview(&state).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut path = None;
    let result = export_with_delivery(
        &state,
        &plan.plan_id,
        || Ok(root.path().to_owned()),
        |root, bytes| {
            let receipt = write_prepared(root, bytes)?;
            path = Some(receipt.directory.clone());
            state.client_core_configuration_committed(Some(&temporary_profile()));
            state.client_core_configuration_committed(None);
            assert_eq!(state.client_core_snapshot().scope, original.scope);
            assert!(state.client_core_snapshot().scope_generation.0 > original.scope_generation.0);
            Ok(receipt)
        },
    );
    let error = result.err().unwrap();
    assert_eq!(error.code, "internal");
    assert!(error.message.contains("未确认"));
    let bytes = fs::read(Path::new(&path.unwrap()).join(FILE_NAME)).unwrap();
    assert_eq!(bytes.len(), plan.bytes as usize);
    assert!(export_with_root(&state, &plan.plan_id, || panic!(
        "unknown delivery replayed"
    ))
    .is_err());
}

#[test]
fn ordinary_observation_revision_advances_without_invalidating_fixed_scope_generation() {
    let state = AppState::default();
    let original = state.client_core_snapshot();
    let plan = diagnostics::preview(&state).unwrap();
    let root = tempfile::tempdir().unwrap();
    state.set_client_core_source_status(crate::client_core::SourceStatus::Degraded);
    assert!(state.client_core_snapshot().revision.0 > original.revision.0);
    assert_eq!(
        state.client_core_snapshot().scope_generation,
        original.scope_generation
    );
    let receipt = export_with_delivery(
        &state,
        &plan.plan_id,
        || Ok(root.path().to_owned()),
        |root, bytes| {
            state.set_client_core_source_status(crate::client_core::SourceStatus::Ready);
            write_prepared(root, bytes)
        },
    )
    .unwrap();
    assert!(Path::new(&receipt.directory).join(FILE_NAME).is_file());
}

#[test]
fn returned_a_scope_cannot_reuse_a_plan_prepared_before_the_b_generation() {
    let state = AppState::default();
    let original = state.client_core_snapshot();
    let plan = diagnostics::preview(&state).unwrap();
    state.client_core_configuration_committed(Some(&temporary_profile()));
    state.client_core_configuration_committed(None);
    assert_eq!(state.client_core_snapshot().scope, original.scope);
    let result = export_with_root(&state, &plan.plan_id, || panic!("old A plan reached IO"));
    assert_eq!(result.err().unwrap().code, "conflict");
    assert!(diagnostics::take(&state, &plan.plan_id).is_err());
}
