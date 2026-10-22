use super::*;
fn profile(id: &str, active: bool) -> ProxyConfigProfile {
    serde_json::from_value(json!({"id":id,"name":id,"kernel":"zero","format":"json","path":null,"content":{"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}]},"active":active,"updatedAtUnixMs":u64::MAX,"capabilities":{ "hasProxyNodes":false,"hasProxyGroups":false,"hasRouteRules":false,"hasRuleSets":false,"hasSelector":false,"hasUrlTest":false,"featureKeys":[] }})).unwrap()
}
fn subscription(id: &str, target: &str) -> SubscriptionProfile {
    serde_json::from_value(json!({"id":id,"name":id,"url":"https://example.invalid/sub?token=fixture","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":target,"updatedAtUnixMs":u64::MAX,"lastSyncAtUnixMs":null,"lastError":null})).unwrap()
}
fn fixture() -> Local {
    Local {
        profiles: vec![
            profile("current", true),
            profile("first", false),
            profile("chosen", false),
        ],
        subscriptions: vec![
            subscription("related", "current"),
            subscription("other", "first"),
        ],
        app: AppConfig::default(),
        scope: json!({"profileId":"current","configRevision":u64::MAX.to_string(),"coreInstanceId":3}),
    }
}
fn input(action: RemovalAction) -> RemovalInput {
    RemovalInput {
        target_id: "current".into(),
        guard: "unused".into(),
        action,
    }
}
#[test]
fn replacement_is_fixed_not_first_and_only_target_links_are_detached() {
    let before = fixture();
    let (p, s) = candidate(
        &before,
        &input(RemovalAction::Replace {
            replacement_id: "chosen".into(),
        }),
    )
    .unwrap();
    assert_eq!(p.iter().find(|p| p.active).unwrap().id, "chosen");
    assert!(!p.iter().any(|p| p.id == "current"));
    assert!(s[0].target_proxy_config_id.is_none());
    assert_eq!(s[0].url, before.subscriptions[0].url);
    assert_eq!(s[1].target_proxy_config_id.as_deref(), Some("first"));
    assert_eq!(before.profiles[0].id, "current");
    assert_eq!(
        before.subscriptions[0].target_proxy_config_id.as_deref(),
        Some("current")
    );
}
#[test]
fn stop_then_database_reload_then_inactive_edit_does_not_resurrect_first() {
    let before = fixture();
    let (profiles, subscriptions) = candidate(&before, &input(RemovalAction::Stop {})).unwrap();
    assert!(profiles.iter().all(|p| !p.active));
    let dir = tempfile::tempdir().unwrap();
    crate::services::app_database::save_domain_data(dir.path(), &profiles, &subscriptions).unwrap();
    let data = crate::services::domain_store::load_all_from_dir(dir.path()).unwrap();
    assert!(data.proxy_configs.iter().all(|p| !p.active));
    let restarted = AppState::with_domain_data(
        AppConfig::default(),
        data.proxy_configs,
        data.subscriptions,
        vec![],
        vec![],
    );
    assert!(restarted.client_core_snapshot().scope.profile_id.is_none());
    let loaded = lock(restarted.proxy_configs(), "proxy_config")
        .unwrap()
        .clone();
    assert!(loaded.iter().all(|p| !p.active));
    let (edited, _) = crate::services::proxy_config::build_upsert_profiles(
        &loaded,
        crate::models::proxy_config::ProxyConfigUpsert {
            id: Some("first".into()),
            name: "edited".into(),
            kernel: None,
            format: None,
            path: None,
            content: Some(json!({})),
            active: None,
        },
    )
    .unwrap();
    assert!(edited.iter().all(|p| !p.active));
}
#[test]
fn sqlite_failure_keeps_previous_sources_links_and_active_marker() {
    let before = fixture();
    let dir = tempfile::tempdir().unwrap();
    crate::services::app_database::save_domain_data(
        dir.path(),
        &before.profiles,
        &before.subscriptions,
    )
    .unwrap();
    let (next, mut links) = candidate(&before, &input(RemovalAction::Stop {})).unwrap();
    links[0].target_proxy_config_id = Some("missing".into());
    assert!(crate::services::app_database::save_domain_data(dir.path(), &next, &links).is_err());
    let data = crate::services::domain_store::load_all_from_dir(dir.path()).unwrap();
    assert_eq!(
        serde_json::to_value(data.proxy_configs).unwrap(),
        serde_json::to_value(&before.profiles).unwrap()
    );
    assert_eq!(
        serde_json::to_value(data.subscriptions).unwrap(),
        serde_json::to_value(&before.subscriptions).unwrap()
    );
}
#[test]
fn proof_changes_for_new_relationship_source_override_scope_and_runtime() {
    let mut local = fixture();
    let runtime = RemovalRuntime {
        state: "running".into(),
        core_instance_id: Some("core".into()),
        config_revision: Some(u64::MAX.to_string()),
        managed_pid: Some(2),
    };
    let first = guard(&local, &runtime, "current").unwrap();
    local.subscriptions.push(subscription("added", "current"));
    assert_ne!(first, guard(&local, &runtime, "current").unwrap());
    let mut local = fixture();
    local.profiles[2].content = Some(json!({"new":true}));
    assert_ne!(first, guard(&local, &runtime, "current").unwrap());
    let mut local = fixture();
    local.app.profile_edits.insert(
        "chosen".into(),
        std::collections::BTreeMap::from([("dns".into(), json!({}))]),
    );
    assert_ne!(first, guard(&local, &runtime, "current").unwrap());
    let mut local = fixture();
    local.scope["coreInstanceId"] = json!(4);
    assert_ne!(first, guard(&local, &runtime, "current").unwrap());
    let changed = RemovalRuntime {
        core_instance_id: Some("new".into()),
        ..runtime
    };
    assert_ne!(first, guard(&fixture(), &changed, "current").unwrap());
}
#[test]
fn native_active_unknown_and_external_ownership_are_blocked_but_inactive_local_is_allowed() {
    let current = profile("current", true);
    let inactive = profile("inactive", false);
    for state in ["platform_unknown", "unknown", "external"] {
        let r = RemovalRuntime {
            state: state.into(),
            core_instance_id: None,
            config_revision: None,
            managed_pid: None,
        };
        assert!(active_reason(&current, &r).is_some());
        assert!(active_reason(&inactive, &r).is_none());
    }
    assert!(!offline_error(&AppError {
        code: "timeout",
        message: "timeout".into(),
        details: None
    }));
}
#[test]
fn protected_ownership_and_invalid_actions_cannot_remove_material() {
    let mut before = fixture();
    before.profiles[0].managed_source =
        Some(crate::models::subscription::ManagedSubscriptionSource {
            plugin_id: "p".into(),
            provider_id: "s".into(),
            remote_subscription_id: "r".into(),
            revision: None,
            source_name: None,
        });
    assert!(candidate(&before, &input(RemovalAction::Stop {})).is_err());
    assert!(candidate(
        &fixture(),
        &input(RemovalAction::Replace {
            replacement_id: "current".into()
        })
    )
    .is_err());
    assert!(candidate(&fixture(), &input(RemovalAction::Inactive {})).is_err());
}
#[test]
fn ambiguity_classification_keeps_failed_capture_recovery_unconfirmed() {
    assert!(uncertain(&AppError {
        code: "permission_denied",
        message: "refused".into(),
        details: Some(json!({"captureRollback":{"succeeded":false}}))
    }));
    assert!(!uncertain(&AppError {
        code: "permission_denied",
        message: "refused".into(),
        details: None
    }));
}
#[test]
fn guest_windows_have_no_preview_commit_or_reconcile_authority() {
    assert!(require_host("main").is_ok());
    assert_eq!(require_host("plugin-guest").unwrap_err().code, "forbidden");
    assert!(!offline_error(&AppError {
        code: "connection_closed",
        message: "unknown".into(),
        details: None
    }));
}
#[test]
fn ordinary_runtime_errors_do_not_echo_subscription_or_proxy_material() {
    let error = safe_runtime_error(
        AppError {
            code: "invalid_argument",
            message: "password=fixture-token".into(),
            details: Some(json!({"raw":"fixture-token"})),
        },
        "replacement_validation",
    );
    assert_eq!(error.code, "invalid_argument");
    assert!(!error.message.contains("fixture-token"));
    assert!(!error.details.unwrap().to_string().contains("fixture-token"));
}
