use super::*;
use crate::models::logs::{HostDiagnosticEvent, ProcessEvent};
fn row(id: u64) -> LogEntry {
    LogEntry {
        id,
        occurred_at_unix_ms: 100,
        source: LogSource::App,
        level: LogLevel::Info,
        message: "password=secret-canary".into(),
        fields: Some(json!({"message":"secret-canary","count":123456789})),
        diagnostic_event: None,
    }
}
fn request(ids: &[u64]) -> LogExportRequest {
    LogExportRequest {
        items: ids
            .iter()
            .map(|id| SelectedLog {
                id: id.to_string(),
                occurred_at_unix_ms: "100".into(),
            })
            .collect(),
        source: Some(LogSource::App),
        level: Some(LogLevel::Info),
        search_applied: true,
        from_unix_ms: Some("100".into()),
        until_unix_ms: Some("100".into()),
        partial: true,
    }
}
fn fixture(action: impl FnOnce(&AppState)) {
    let dir = tempfile::tempdir().unwrap();
    super::super::log_store::with_test_path(&dir.path().join("logs.jsonl"), || {
        let state = AppState::default();
        action(&state)
    });
}
#[test]
fn selection_enforces_exact_id_timestamp_and_typed_filters() {
    assert!(selection(&request(&[1]), vec![row(1)]).is_ok());
    assert!(selection(&request(&[2]), vec![row(1)]).is_err());
    for changed in ["timestamp", "source", "level", "from", "until"] {
        let mut r = request(&[1]);
        match changed {
            "timestamp" => r.items[0].occurred_at_unix_ms = "101".into(),
            "source" => r.source = Some(LogSource::Core),
            "level" => r.level = Some(LogLevel::Error),
            "from" => r.from_unix_ms = Some("101".into()),
            _ => r.until_unix_ms = Some("99".into()),
        };
        assert!(selection(&r, vec![row(1)]).is_err(), "{changed}");
    }
}
#[test]
fn empty_duplicate_noncanonical_and_excessive_selection_are_rejected() {
    for ids in [vec![], vec![1, 1], vec![1; 2001]] {
        assert!(selection(&request(&ids), vec![row(1)]).is_err());
    }
    for value in ["01", "-1", "18446744073709551616", "secret"] {
        let mut r = request(&[1]);
        r.items[0].id = value.into();
        assert!(selection(&r, vec![row(1)]).is_err());
    }
    let mut max = request(&(1..=2000).collect::<Vec<_>>());
    max.from_unix_ms = None;
    max.until_unix_ms = None;
    assert_eq!(
        selection(&max, (1..=2000).map(row).collect())
            .unwrap()
            .len(),
        2000
    );
}
#[test]
fn real_private_store_lookup_and_shared_projection_omit_unknown_body_and_free_search() {
    fixture(|state| {
        let mut ordinary = row(1);
        ordinary.diagnostic_event = Some(HostDiagnosticEvent::Process(ProcessEvent::Started));
        super::super::log_store::append(&ordinary, 5000).unwrap();
        super::super::log_store::append(&row(2), 5000).unwrap();
        let preview = preview(state, request(&[1, 2])).unwrap();
        assert_eq!(preview.count, 2);
        assert_eq!(preview.classified_count, 1);
        assert_eq!(preview.unclassified_count, 1);
        assert!(
            diagnostics::take(state, &preview.plan_id).is_err(),
            "diagnostic endpoint cannot consume a log plan"
        );
        let bytes = diagnostics::take_selected(state, &preview.plan_id).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("secret-canary"));
        assert!(!text.contains("123456789"));
        let document: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(document["schemaVersion"], "znet.mobile-diagnostics.v3");
        assert_eq!(document["logs"][0]["safeMessage"], "内核进程已启动");
        assert_eq!(document["logs"][1]["messageOmitted"], true);
        assert_eq!(document["filter"]["searchApplied"], true);
        assert!(document["filter"].get("search").is_none());
        assert!(diagnostics::take_selected(state, &preview.plan_id).is_err());
    });
}
#[test]
fn snapshot_is_fixed_and_old_scope_or_expired_plan_never_delivers() {
    fixture(|state| {
        super::super::log_store::append(&row(1), 5000).unwrap();
        let frozen = preview(state, request(&[1])).unwrap();
        super::super::log_store::append(&row(2), 5000).unwrap();
        let bytes = diagnostics::take_selected(state, &frozen.plan_id).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(document["logs"].as_array().unwrap().len(), 1);
        let late = preview(state, request(&[1])).unwrap();
        state.client_core_instance_started();
        assert!(diagnostics::take_selected(state, &late.plan_id).is_err());
        let scope = state.client_core_snapshot().scope;
        let (expired, _) = diagnostics::store_selected(
            state,
            scope,
            state.client_core_snapshot().scope_generation.0,
            b"{}".to_vec(),
            0,
        )
        .unwrap();
        assert!(diagnostics::take_selected(state, &expired).is_err());
    });
}
#[test]
fn source_mismatch_is_metadata_only_and_unknown_input_fields_are_not_accepted() {
    let mut entry = row(1);
    entry.source = LogSource::Plugin;
    entry.diagnostic_event = Some(HostDiagnosticEvent::Process(ProcessEvent::Started));
    assert!(diagnostics::log(&entry)["safeMessage"].is_null());
    assert!(serde_json::from_value::<LogExportRequest>(json!({"items":[],"source":null,"level":null,"searchApplied":false,"fromUnixMs":null,"untilUnixMs":null,"partial":false,"message":"secret-canary"})).is_err());
}
#[test]
fn final_document_budget_is_enforced() {
    assert!(diagnostics::bounded_document(&json!({"oversized":"x".repeat(2*1024*1024)})).is_err());
}
#[test]
fn dismissing_only_unsubmitted_log_plans_releases_bounded_capacity() {
    fixture(|state| {
        super::super::log_store::append(&row(1), 5000).unwrap();
        let plans = (0..4)
            .map(|_| preview(state, request(&[1])).unwrap().plan_id)
            .collect::<Vec<_>>();
        assert!(preview(state, request(&[1])).is_err());
        diagnostics::discard_selected(state, &plans[0]).unwrap();
        assert!(preview(state, request(&[1])).is_ok());
        assert!(diagnostics::take_selected(state, &plans[0]).is_err());
    });
}
