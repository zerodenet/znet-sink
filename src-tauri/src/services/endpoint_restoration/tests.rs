use super::*;
use std::sync::Mutex;
fn source() -> Value {
    json!({"endpoints":[{"tag":"a","enabled":true,"directions":{"inbound":false,"outbound":true},"protocol":{"type":"wireguard"}}]})
}
fn row() -> Value {
    json!({"endpoint_id":"opaque:a","tag":"a","protocol":"wireguard","inbound_tags":[],"outbound_tags":["a"],"configuration":{"origin":"canonical","enabled":true,"directions":{"inbound":false,"outbound":true}},"core_instance_id":"core-a","config_revision":4,"intent_revision":7,"generation":1,"enabled":false,"allowed":{"inbound":false,"outbound":true},"state":"running","state_source":"runtime_override","supported":{"directions":{"inbound":false,"outbound":true},"operations":["clear_overrides"]}})
}
pub(super) fn record() -> Record {
    let target = RestorationTarget {
        binding_id: "bound".into(),
        endpoint_id: "opaque:a".into(),
        profile_id: "p".into(),
        tag: "a".into(),
        protocol: "wireguard".into(),
    };
    let plan = RestorationPlan {
        plan_id: "operation".into(),
        target: target.clone(),
        source_digest: "source-digest".into(),
        source_updated_at_unix_ms: "1".into(),
        previous_effective_digest: "before".into(),
        effective_digest: "after".into(),
        removed_fields: vec!["enabled".into()],
        runtime_available: true,
        impact: "preview".into(),
        client_scope: AppState::default().client_core_snapshot().scope,
        created_at_unix_ms: "1000".into(),
        expires_at_unix_ms: "301000".into(),
    };
    Record {
        receipt: receipt_for(&plan),
        plan,
        binding: Binding {
            target,
            source_digest: "source-digest".into(),
            source_updated_at: 1,
            receipt_digest: "before".into(),
            snapshot: row(),
        },
        app_digest: "app".into(),
        overlay: json!({"protocol":"wireguard","enabled":false}),
        clear_started: false,
    }
}
struct Fake {
    row: Mutex<Value>,
    calls: Mutex<Vec<String>>,
    fail_apply: bool,
    fail_clear: bool,
    fail_publish: bool,
    changed_after_apply: bool,
}
impl Fake {
    fn new() -> Self {
        Self {
            row: Mutex::new(row()),
            calls: Mutex::new(vec![]),
            fail_apply: false,
            fail_clear: false,
            fail_publish: false,
            changed_after_apply: false,
        }
    }
    fn note(&self, call: &str) {
        self.calls.lock().unwrap().push(call.into())
    }
}
impl Backend for Fake {
    async fn endpoint(&self, _: &str) -> AppResult<Value> {
        self.note("read");
        Ok(self.row.lock().unwrap().clone())
    }
    async fn validate(&self, _: &Value) -> AppResult<()> {
        self.note("validate");
        Ok(())
    }
    async fn apply(&self, _: &Value, _: &Value) -> AppResult<RuntimeReceipt> {
        self.note("apply");
        if self.fail_apply {
            return Err(AppError::internal("lost reply"));
        }
        let mut row = self.row.lock().unwrap();
        row["config_revision"] = json!(5);
        row["intent_revision"] = json!(8);
        if self.changed_after_apply {
            row["endpoint_id"] = json!("replacement")
        }
        Ok(RuntimeReceipt {
            core_instance_id: "core-a".into(),
            config_revision: "5".into(),
        })
    }
    async fn clear(&self, row: &Value) -> AppResult<Value> {
        self.note("clear");
        assert_eq!(row["intent_revision"], 8);
        assert_eq!(row["config_revision"], 5);
        if self.fail_clear {
            return Err(AppError::internal("lost reply"));
        }
        let mut row = self.row.lock().unwrap();
        row["state_source"] = json!("config");
        row["enabled"] = json!(true);
        Ok(row.clone())
    }
    fn checkpoint(&self, record: &Record) -> AppResult<()> {
        self.note(&format!("save:{:?}", record.receipt.phase));
        Ok(())
    }
    fn publish(&self, _: &AppConfig, _: AppConfig) -> AppResult<()> {
        self.note("publish");
        if self.fail_publish {
            Err(conflict("CAS refusal"))
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn restores_only_after_real_apply_clear_receipts_with_new_revision_and_fixed_resource() {
    let backend = Fake::new();
    let mut record = record();
    let receipt = execute(
        &backend,
        &mut record,
        &source(),
        &AppConfig::default(),
        AppConfig::default(),
        &source(),
    )
    .await
    .unwrap();
    assert_eq!(receipt.phase, RestorePhase::Confirmed);
    assert!(receipt.runtime_confirmed && receipt.local_saved);
    let calls = backend.calls.lock().unwrap();
    assert!(
        calls.iter().position(|v| v == "save:Applying").unwrap()
            < calls.iter().position(|v| v == "apply").unwrap()
    );
    assert!(
        calls.iter().position(|v| v == "clear").unwrap()
            < calls.iter().position(|v| v == "publish").unwrap()
    );
}
#[tokio::test]
async fn lost_apply_reply_cannot_clear_publish_or_claim_rollback() {
    let backend = Fake {
        fail_apply: true,
        ..Fake::new()
    };
    let mut record = record();
    let result = execute(
        &backend,
        &mut record,
        &source(),
        &AppConfig::default(),
        AppConfig::default(),
        &source(),
    )
    .await
    .unwrap();
    assert_eq!(result.phase, RestorePhase::ResultUnknown);
    assert!(!result.runtime_confirmed && !result.local_saved);
    assert!(!record.clear_started);
    assert!(!backend
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|v| v == "clear" || v == "publish"));
}
#[tokio::test]
async fn lost_clear_reply_and_publication_conflict_keep_stages_distinct_without_replay() {
    for failure in ["clear", "publish"] {
        let backend = Fake {
            fail_clear: failure == "clear",
            fail_publish: failure == "publish",
            ..Fake::new()
        };
        let mut record = record();
        let result = execute(
            &backend,
            &mut record,
            &source(),
            &AppConfig::default(),
            AppConfig::default(),
            &source(),
        )
        .await
        .unwrap();
        assert_eq!(result.phase, RestorePhase::ResultUnknown);
        assert!(!result.local_saved);
        assert_eq!(result.runtime_confirmed, failure == "publish");
        assert_eq!(
            backend
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|v| *v == "apply")
                .count(),
            1
        );
        assert_eq!(
            backend
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|v| *v == "clear")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn replacement_after_apply_cannot_receive_old_clear() {
    let backend = Fake {
        changed_after_apply: true,
        ..Fake::new()
    };
    let mut record = record();
    let result = execute(
        &backend,
        &mut record,
        &source(),
        &AppConfig::default(),
        AppConfig::default(),
        &source(),
    )
    .await
    .unwrap();
    assert_eq!(result.phase, RestorePhase::ResultUnknown);
    assert!(!backend
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|v| v == "clear" || v == "publish"));
}
#[tokio::test]
async fn changed_generation_before_submit_cannot_start_any_mutation() {
    let backend = Fake::new();
    backend.row.lock().unwrap()["generation"] = json!(2);
    let mut record = record();
    assert!(execute(
        &backend,
        &mut record,
        &source(),
        &AppConfig::default(),
        AppConfig::default(),
        &source()
    )
    .await
    .is_err());
    assert_eq!(*backend.calls.lock().unwrap(), vec!["read"]);
}
#[test]
fn raw_snapshot_cache_excludes_unknown_protocol_material_and_private_fields() {
    let mut row = row();
    row["private_key"] = json!("must-not-persist");
    row["details"] = json!({"secret":"must-not-persist"});
    row["configuration"]["private_key"] = json!("must-not-persist");
    assert!(!public_snapshot(&row)
        .to_string()
        .contains("must-not-persist"));
}

#[test]
fn offline_pending_is_persisted_before_local_save_and_never_calls_kernel() {
    let backend = Fake::new();
    let mut record = record();
    let receipt = save_pending(
        &backend,
        &mut record,
        &AppConfig::default(),
        AppConfig::default(),
    )
    .unwrap();
    assert_eq!(receipt.phase, RestorePhase::SavedPending);
    assert!(receipt.local_saved);
    assert!(!receipt.runtime_confirmed);
    assert_eq!(
        *backend.calls.lock().unwrap(),
        vec!["save:SavedPending", "publish", "save:SavedPending"]
    );
}
#[test]
fn offline_cas_refusal_never_claims_saved_or_runtime_success() {
    let backend = Fake {
        fail_publish: true,
        ..Fake::new()
    };
    let mut record = record();
    let receipt = save_pending(
        &backend,
        &mut record,
        &AppConfig::default(),
        AppConfig::default(),
    )
    .unwrap();
    assert_eq!(receipt.phase, RestorePhase::Rejected);
    assert!(!receipt.local_saved && !receipt.runtime_confirmed);
    assert!(!backend
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|v| v == "apply" || v == "clear"));
}

#[test]
fn prepared_plan_rejects_changed_client_scope_and_expiry_for_all_modes() {
    let record = record();
    let scope = record.plan.client_scope.clone();
    assert!(validate_plan(&record.plan, &scope, 1001).is_ok());
    assert!(validate_plan(&record.plan, &scope, 301000).is_err());
    assert!(validate_plan(&record.plan, &scope, 999).is_err());
    let mut changed = scope;
    changed.config_revision.0 += 1;
    assert!(validate_plan(&record.plan, &changed, 1001).is_err());
}

#[test]
fn pending_local_save_rebinds_only_to_exact_previous_app_and_receipt() {
    let mut record = record();
    record.receipt.phase = RestorePhase::SavedPending;
    let mut original = AppConfig::default();
    original
        .profile_edits
        .entry("p".into())
        .or_default()
        .insert("endpoints".into(), json!({"a": record.overlay.clone()}));
    record.app_digest = digest(&original).unwrap();
    record.binding.source_digest = digest(&source()).unwrap();
    let pending_app = endpoints::restore_candidate(&original, "p", &source(), &row()).unwrap();
    assert!(pending_receipt_matches(&record, &pending_app, "p", &source(), 1, "before").unwrap());
    assert!(!pending_receipt_matches(&record, &pending_app, "p", &source(), 1, "wrong").unwrap());
    assert!(!pending_receipt_matches(&record, &pending_app, "p", &source(), 2, "before").unwrap());
    let mut unrelated_change = pending_app;
    unrelated_change
        .profile_edits
        .entry("p".into())
        .or_default()
        .insert("dns".into(), json!({"mode":"changed"}));
    assert!(
        !pending_receipt_matches(&record, &unrelated_change, "p", &source(), 1, "before").unwrap()
    );
}

#[test]
fn missing_source_defaults_never_confirm_a_malformed_snapshot() {
    let source = json!({"endpoints":[{"tag":"a","protocol":{"type":"wireguard"}}]});
    let mut valid = row();
    valid["state_source"] = json!("config");
    valid["enabled"] = valid["configuration"]["enabled"].clone();
    assert!(restored_intent(&valid, &source));
    for field in ["enabled", "allowed", "configuration"] {
        let mut malformed = valid.clone();
        malformed.as_object_mut().unwrap().remove(field);
        assert!(!restored_intent(&malformed, &source));
    }
}
