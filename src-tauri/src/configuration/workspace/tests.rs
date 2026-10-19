use super::*;
use crate::models::{
    app_config::AppConfig,
    proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile},
    subscription::ManagedSubscriptionSource,
};
fn source() -> Value {
    json!({"route":{"final":{"type":"direct"}},"runtime":{"udp_upstream_idle_timeout_seconds":18446744073709551615u64}})
}
fn profile() -> ProxyConfigProfile {
    ProxyConfigProfile {
        policy_selections: Default::default(),
        id: "ordinary".into(),
        name: "ordinary".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(source()),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: ProxyConfigCapabilities::default(),
    }
}
pub(crate) fn state() -> AppState {
    let mut app = AppConfig::default();
    app.routing.inject_common_rules = false;
    crate::services::bypass::normalize(&mut app).unwrap();
    let state = AppState::with_domain_data(app, vec![profile()], vec![], vec![], vec![]);
    let dir = tempfile::tempdir().unwrap().keep();
    crate::services::app_database::save_domain_data(
        &dir,
        &state.proxy_configs().lock().unwrap(),
        &[],
    )
    .unwrap();
    crate::services::app_config_store::save(
        &dir.join("app-config.json"),
        &state.app_config().lock().unwrap(),
    )
    .unwrap();
    *state.configuration().plans.1.lock().unwrap() = Some(dir);
    state
}
fn input(state: &AppState, source: Value) -> ConfigWorkspaceDraftInput {
    ConfigWorkspaceDraftInput {
        binding: local_snapshot(state).unwrap().binding.unwrap(),
        source_json: source.to_string(),
    }
}
#[test]
fn native_schema_and_all_references_are_checked_without_rewriting_u64() {
    let value = parse(&source().to_string()).unwrap();
    assert!(pretty(&value).unwrap().contains("18446744073709551615"));
    for value in [
        json!({}),
        json!({"schema_version":999,"route":{"final":{"type":"direct"}}}),
        json!({"route":{"final":{"type":"route","outbound":"secret-canary"}}}),
        json!({"route":{"final":{"type":"direct"}},"runtime":{"dns":{"servers":{"x":{"type":"secret-canary"}}}}}),
    ] {
        let error = parse(&value.to_string()).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("secret-canary"));
        assert!(error.details.unwrap()["errors"].is_array());
    }
}
#[test]
fn malformed_json_has_real_line_column_without_echoing_input() {
    let error = parse("{\n\"secret-canary\":").unwrap_err();
    assert_eq!(error.details.as_ref().unwrap()["errors"][0]["line"], 2);
    assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
}
#[test]
fn managed_and_unavailable_material_snapshots_contain_no_views_and_all_preparation_is_denied() {
    for managed in [true, false] {
        let mut p = profile();
        if managed {
            p.managed_source = Some(ManagedSubscriptionSource {
                plugin_id: "plugin".into(),
                provider_id: "provider".into(),
                remote_subscription_id: "source".into(),
                revision: None,
                source_name: None,
            });
            p.content = Some(json!({"secret-canary":"secret"}));
        } else {
            p.content = None;
        }
        let state =
            AppState::with_domain_data(AppConfig::default(), vec![p], vec![], vec![], vec![]);
        let snapshot = local_snapshot(&state).unwrap();
        assert!(snapshot.read_only_reason.is_some());
        assert!(snapshot.binding.is_none());
        assert!(
            snapshot.source_json.is_none()
                && snapshot.local_edits_json.is_none()
                && snapshot.effective_json.is_none()
        );
        assert!(!serde_json::to_string(&snapshot)
            .unwrap()
            .contains("secret-canary"));
        assert!(from_input(
            &state,
            &ConfigWorkspaceDraftInput {
                binding: "fake".into(),
                source_json: source().to_string()
            }
        )
        .is_err());
        assert!(save(
            &state,
            ConfigWorkspaceDraftInput {
                binding: "fake".into(),
                source_json: source().to_string()
            }
        )
        .is_err());
    }
}
#[test]
fn source_same_timestamp_app_rules_and_a_b_a_invalidate_original_binding() {
    for kind in ["source", "app", "rules", "aba"] {
        let state = state();
        let input = input(&state, source());
        match kind {
            "source" => {
                state.proxy_configs().lock().unwrap()[0].content =
                    Some(json!({"route":{"final":{"type":"reject"}}}))
            }
            "app" => state.app_config().lock().unwrap().ui.theme = "dark".into(),
            "rules" => {
                let changed=serde_json::from_value(json!({"id":"r","name":"r","enabled":false,"semanticIr":{"version":1,"rules":[]},"updatedAtUnixMs":1})).unwrap();
                state.rule_sets().lock().unwrap().push(changed);
            }
            _ => {
                state.client_core_configuration_committed(None);
                state.client_core_configuration_committed(Some(&profile()));
            }
        }
        assert!(from_input(&state, &input).is_err(), "{kind}");
    }
}
#[test]
fn local_publication_reopens_sqlite_preserves_runtime_binding_and_does_not_confirm_runtime() {
    let state = state();
    let dir = tempfile::tempdir().unwrap();
    state
        .app_config()
        .lock()
        .unwrap()
        .local_proxy
        .source_proxy_config_id = Some("runtime-owner".into());
    let (context, source, _) = from_input(&state, &input(&state, source())).unwrap();
    let mut next = context.profiles.clone();
    next[0].content = Some(source);
    next[0].updated_at_unix_ms = 2;
    crate::configuration::persistence::commit_checked(
        &state,
        &context.profiles,
        &context.subscriptions,
        &context.app,
        next,
        context.subscriptions.clone(),
        true,
        Some(dir.path()),
        Some(&context.binding.rules_digest),
        Some((&context.binding.scope, context.binding.generation)),
    )
    .unwrap();
    let stored = crate::services::domain_store::load_all_from_dir(dir.path()).unwrap();
    assert_eq!(
        stored.proxy_configs[0].content.as_ref().unwrap()["runtime"]
            ["udp_upstream_idle_timeout_seconds"]
            .as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        state
            .app_config()
            .lock()
            .unwrap()
            .local_proxy
            .source_proxy_config_id
            .as_deref(),
        Some("runtime-owner")
    );
    assert!(state.configuration().transaction().is_none());
    let reopened = AppState::with_domain_data(
        crate::services::app_config_store::load_or_default(&dir.path().join("app-config.json"))
            .unwrap(),
        stored.proxy_configs,
        stored.subscriptions,
        vec![],
        vec![],
    );
    recovery::set_test_directory(&reopened, dir.path().to_path_buf());
    let snapshot = local_snapshot(&reopened).unwrap();
    assert!(snapshot.pending_apply);
    assert!(!snapshot.runtime.confirmed);
    assert!(snapshot.runtime.running.is_none());
}
#[test]
fn runtime_identity_and_transaction_id_are_lossless_decimal_wire_values() {
    let receipt = ConfigTransactionReceipt {
        transaction_id: u64::MAX,
        profile_id: "p".into(),
        strategy: ConfigApplyStrategy::HotReload,
        state: "confirmed",
        effective_digest: "digest".into(),
        started_at_unix_ms: 1,
        completed_at_unix_ms: 2,
        runtime_identity: ConfigRuntimeIdentityView {
            core_instance_id: "core".into(),
            config_revision: u64::MAX,
        },
    };
    let value = serde_json::to_value(receipt).unwrap();
    assert_eq!(value["transactionId"], u64::MAX.to_string());
    assert_eq!(
        value["runtimeIdentity"]["configRevision"],
        u64::MAX.to_string()
    );
}

pub(crate) fn prepared(state: &AppState) -> Prepared {
    let (context, source, effective) = from_input(state, &input(state, source())).unwrap();
    let capture = crate::configuration::capture_apply::candidate_capture_parameters(
        state,
        &source,
        Some(&context.profile.id),
    )
    .unwrap();
    Prepared {
        binding: context.binding.clone(),
        source: source.clone(),
        effective: effective.clone(),
        capture_digest: digest(&capture).unwrap(),
        frozen_capture_parameters: capture,
        identity: Some(crate::kernel::zero::queries::KernelRuntimeIdentity {
            core_instance_id: "core".into(),
            config_revision: 1,
        }),
        confirmed_binding: None,
        plan: ConfigWorkspacePlan {
            impact: crate::models::gui_core::GuiConfigPlanApplyResult {
                valid: true,
                hot_reload: vec![],
                requires_restart: vec![],
                warnings: vec![],
                errors: vec![],
            },
            plan_id: String::new(),
            binding: digest(&context.binding).unwrap(),
            source_json: pretty(&source).unwrap(),
            effective_json: pretty(&effective).unwrap(),
            effective_digest: digest(&effective).unwrap(),
            expires_at_unix_ms: String::new(),
            runtime_validated: true,
            apply_unavailable_reason: None,
        },
        operation: ConfigWorkspaceOperation {
            plan_id: String::new(),
            state: "prepared",
            receipt: None,
            reason: None,
        },
    }
}
#[test]
fn plan_ttl_single_submission_and_unknown_barrier_never_replay_or_use_old_same_digest_receipt() {
    let state = state();
    let plan = store(&state, prepared(&state)).unwrap();
    {
        state
            .configuration()
            .plans
            .0
            .lock()
            .unwrap()
            .get_mut(&plan.plan_id)
            .unwrap()
            .plan
            .expires_at_unix_ms = "0".into();
    }
    assert!(submit(&state, &plan.plan_id).is_err());
    discard(&state, &plan.plan_id).unwrap();
    let plan = store(&state, prepared(&state)).unwrap();
    submit(&state, &plan.plan_id).unwrap();
    assert!(submit(&state, &plan.plan_id).is_err());
    assert!(from_input(&state, &input(&state, source())).is_err());
    discard(&state, &plan.plan_id).unwrap();
    assert_eq!(
        operation(&state, &plan.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert!(operation(&state, "unrelated-same-digest-operation").is_err());
}
#[test]
fn confirmed_history_is_bounded_without_exhausting_new_plan_capacity_or_dropping_unknown() {
    let state = state();
    for _ in 0..8 {
        let plan = store(&state, prepared(&state)).unwrap();
        state
            .configuration()
            .plans
            .0
            .lock()
            .unwrap()
            .get_mut(&plan.plan_id)
            .unwrap()
            .operation
            .state = "confirmed";
    }
    assert_eq!(state.configuration().plans.0.lock().unwrap().len(), 4);
    let plan = store(&state, prepared(&state)).unwrap();
    submit(&state, &plan.plan_id).unwrap();
    assert_eq!(
        operation(&state, &plan.plan_id).unwrap().state,
        "submitted_unknown"
    );
}
#[test]
fn local_save_uses_real_publication_and_projection_failure_restores_sqlite_without_claiming_success(
) {
    let state = state();
    let dir = tempfile::tempdir().unwrap();
    crate::services::domain_store::save_relational_data_to_dir(dir.path(), &[profile()], &[])
        .unwrap();
    std::fs::create_dir(dir.path().join("app-config.json")).unwrap();
    assert!(save_at(&state, input(&state, source()), Some(dir.path())).is_err());
    let reopened = crate::services::domain_store::load_all_from_dir(dir.path()).unwrap();
    assert_eq!(reopened.proxy_configs[0].updated_at_unix_ms, 1);
    assert_eq!(
        state.proxy_configs().lock().unwrap()[0].updated_at_unix_ms,
        1
    );
    assert!(state.configuration().transaction().is_none());
}
#[test]
fn rules_changed_after_validation_fail_under_publication_lock_and_keep_original_profile() {
    let state = state();
    let dir = tempfile::tempdir().unwrap();
    let (context, source, _) = from_input(&state, &input(&state, source())).unwrap();
    let changed=serde_json::from_value(json!({"id":"r","name":"r","enabled":false,"semanticIr":{"version":1,"rules":[]},"updatedAtUnixMs":1})).unwrap();
    state.rule_sets().lock().unwrap().push(changed);
    let mut next = context.profiles.clone();
    next[0].content = Some(source);
    next[0].updated_at_unix_ms = 2;
    assert!(crate::configuration::persistence::commit_checked(
        &state,
        &context.profiles,
        &context.subscriptions,
        &context.app,
        next,
        context.subscriptions.clone(),
        true,
        Some(dir.path()),
        Some(&context.binding.rules_digest),
        Some((&context.binding.scope, context.binding.generation))
    )
    .is_err());
    assert_eq!(state.rule_sets().lock().unwrap().len(), 1);
    assert_eq!(
        state.proxy_configs().lock().unwrap()[0].updated_at_unix_ms,
        1
    );
    assert!(crate::services::domain_store::load_all_from_dir(dir.path())
        .unwrap()
        .proxy_configs
        .is_empty());
}
#[test]
fn actual_local_save_then_sqlite_reopen_is_pending_and_preserves_u64() {
    let state = state();
    let dir = tempfile::tempdir().unwrap();
    let result = save_at(&state, input(&state, source()), Some(dir.path())).unwrap();
    assert_eq!(result.state, "saved_pending_apply");
    assert!(result.snapshot.pending_apply);
    assert!(result.snapshot.runtime.identity.is_none());
    assert!(result
        .snapshot
        .source_json
        .unwrap()
        .contains("18446744073709551615"));
    let data = crate::services::domain_store::load_all_from_dir(dir.path()).unwrap();
    assert!(data.proxy_configs[0].updated_at_unix_ms > 1);
    assert!(state.configuration().transaction().is_none());
}

#[test]
fn earlier_prepared_peer_cannot_bypass_unknown_or_runtime_confirmed_pending_local_barrier() {
    for phase in ["submitted_unknown", "runtime_confirmed_pending_local"] {
        let state = state();
        let a = store(&state, prepared(&state)).unwrap();
        let b = store(&state, prepared(&state)).unwrap();
        submit(&state, &a.plan_id).unwrap();
        if phase == "runtime_confirmed_pending_local" {
            let created = recovery::metadata(&state, &a.plan_id)
                .unwrap()
                .unwrap()
                .created_at_unix_ms;
            runtime_confirmed(
                &state,
                &a.plan_id,
                ConfigTransactionReceipt {
                    transaction_id: 1,
                    profile_id: "ordinary".into(),
                    strategy: ConfigApplyStrategy::HotReload,
                    state: "runtime_confirmed",
                    effective_digest: a.effective_digest.clone(),
                    started_at_unix_ms: created,
                    completed_at_unix_ms: created,
                    runtime_identity: ConfigRuntimeIdentityView {
                        core_instance_id: "core".into(),
                        config_revision: 2,
                    },
                },
            )
            .unwrap();
        }
        assert!(review(&state, &b.plan_id).is_err());
        assert!(submit(&state, &b.plan_id).is_err());
        assert_eq!(operation(&state, &b.plan_id).unwrap().state, "prepared");
        assert_eq!(operation(&state, &a.plan_id).unwrap().state, phase);
    }
}

#[test]
fn pre_side_effect_rejection_cannot_clear_other_phases_or_recorded_receipts() {
    let state = state();
    let plan = store(&state, prepared(&state)).unwrap();
    assert!(reject_without_runtime_mutation(&state, &plan.plan_id).is_err());
    submit(&state, &plan.plan_id).unwrap();
    reject_without_runtime_mutation(&state, &plan.plan_id).unwrap();
    assert_eq!(operation(&state, &plan.plan_id).unwrap().state, "rejected");
    assert!(ensure_unblocked(&state, "ordinary").is_ok());
    assert!(submit(&state, &plan.plan_id).is_err());
    assert!(reject_without_runtime_mutation(&state, &plan.plan_id).is_err());
    for phase in ["runtime_confirmed_pending_local", "confirmed"] {
        state
            .configuration()
            .plans
            .0
            .lock()
            .unwrap()
            .get_mut(&plan.plan_id)
            .unwrap()
            .operation
            .state = phase;
        assert!(reject_without_runtime_mutation(&state, &plan.plan_id).is_err());
    }
    let receipt = ConfigTransactionReceipt {
        transaction_id: 1,
        profile_id: "ordinary".into(),
        strategy: ConfigApplyStrategy::HotReload,
        state: "runtime_confirmed",
        effective_digest: "digest".into(),
        started_at_unix_ms: 1,
        completed_at_unix_ms: 2,
        runtime_identity: ConfigRuntimeIdentityView {
            core_instance_id: "core".into(),
            config_revision: 1,
        },
    };
    {
        let mut plans = state.configuration().plans.0.lock().unwrap();
        let entry = plans.get_mut(&plan.plan_id).unwrap();
        entry.operation.state = "submitted_unknown";
        entry.operation.receipt = Some(receipt);
    }
    assert!(reject_without_runtime_mutation(&state, &plan.plan_id).is_err());
    assert!(state
        .configuration()
        .plans
        .0
        .lock()
        .unwrap()
        .get(&plan.plan_id)
        .unwrap()
        .operation
        .receipt
        .is_some());
}
