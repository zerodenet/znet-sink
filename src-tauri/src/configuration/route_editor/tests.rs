use super::*;
use crate::{
    models::{proxy_config::ProxyConfigCapabilities, subscription::ManagedSubscriptionSource},
    services::{app_config_store, app_database, domain_store},
};

struct Fixture {
    state: AppState,
    dir: tempfile::TempDir,
}
fn source() -> Value {
    json!({"route":{"rules":[{"condition":{"type":"domain","values":["example.com"]},"action":{"type":"direct"},"mode":"flow"}],"final":{"type":"direct"}},"runtime":{"udp_upstream_idle_timeout_seconds":u64::MAX}})
}
fn profile() -> ProxyConfigProfile {
    ProxyConfigProfile {
        id: "manual".into(),
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
fn fixture(mut p: ProxyConfigProfile, subs: Vec<SubscriptionProfile>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut app = AppConfig::default();
    crate::services::bypass::normalize(&mut app).unwrap();
    app.local_proxy.source_proxy_config_id = Some("runtime-owned".into());
    app.core.socket = Some(
        dir.path()
            .join("no-runtime.sock")
            .to_string_lossy()
            .into_owned(),
    );
    p.capabilities = crate::services::proxy_config::analyze_capabilities(p.content.as_ref());
    app_database::save_domain_data(dir.path(), std::slice::from_ref(&p), &subs).unwrap();
    app_config_store::save(&dir.path().join("app-config.json"), &app).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    let state = AppState::with_domain_data(app, vec![p], subs, vec![], vec![]);
    workspace::recovery::set_test_directory(&state, dir.path().to_path_buf());
    Fixture { state, dir }
}
fn draft(f: &Fixture) -> PreviewInput {
    let s = snapshot(&f.state, "manual", false).unwrap();
    PreviewInput {
        profile_id: s.profile_id,
        binding: s.binding.unwrap(),
        mode: Mode::Replace,
        rows: s.rows,
    }
}
fn prepare(f: &Fixture) -> Plan {
    preview(&f.state, draft(f)).unwrap()
}
fn owner() -> ManagedSubscriptionSource {
    ManagedSubscriptionSource {
        plugin_id: "secret-canary".into(),
        provider_id: "provider".into(),
        remote_subscription_id: "remote".into(),
        revision: None,
        source_name: None,
    }
}
fn subscription(managed: bool) -> SubscriptionProfile {
    let mut s:SubscriptionProfile=serde_json::from_value(json!({"id":"sub","name":"sub","url":"https://example.invalid/secret-canary","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":"manual","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap();
    if managed {
        s.managed_source = Some(owner());
    }
    s
}

#[test]
fn saves_only_profile_overlay_reopens_actual_app_file_and_preserves_sqlite_source_u64_and_runtime_owner(
) {
    let f = fixture(profile(), vec![]);
    let before = app_database::load_domain_data(f.dir.path()).unwrap();
    let epoch = workspace_recovery::current_epoch(f.dir.path()).unwrap();
    #[cfg(unix)]
    let listener =
        std::os::unix::net::UnixListener::bind(f.dir.path().join("no-runtime.sock")).unwrap();
    #[cfg(unix)]
    listener.set_nonblocking(true).unwrap();
    let mut input = draft(&f);
    input.rows[0].enabled = false;
    input.rows.push(Row {
        id: "new-1".into(),
        enabled: true,
        rule_json:
            json!({"condition":{"type":"ip","values":["192.0.2.0/24"]},"action":{"type":"reject"}})
                .to_string(),
    });
    let plan = preview(&f.state, input).unwrap();
    let result = save(&f.state, &plan.plan_id).unwrap();
    assert_eq!(result.state, "saved_pending_apply");
    let disk = app_config_store::load_or_default(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        disk.local_proxy.source_proxy_config_id.as_deref(),
        Some("runtime-owned")
    );
    assert_eq!(
        disk.profile_edits["manual"]["routeRules"]["rows"][0]["enabled"],
        false
    );
    let (_, effective) =
        super::super::local_edits::resolve(&disk, Some("manual"), &source()).unwrap();
    assert_eq!(effective["route"]["rules"].as_array().unwrap().len(), 1);
    assert_eq!(
        effective["runtime"]["udp_upstream_idle_timeout_seconds"].as_u64(),
        Some(u64::MAX)
    );
    let after = app_database::load_domain_data(f.dir.path()).unwrap();
    assert_eq!(
        workspace::digest(&before.proxy_configs).unwrap(),
        workspace::digest(&after.proxy_configs).unwrap()
    );
    assert_eq!(
        epoch,
        workspace_recovery::current_epoch(f.dir.path()).unwrap()
    );
    assert!(f.state.configuration().transaction().is_none());
    #[cfg(unix)]
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let history = app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &plan.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    assert_eq!(
        history,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
}
#[test]
fn managed_profile_and_managed_relation_never_expose_rows_even_with_explicit_from_source() {
    for linked in [false, true] {
        let mut p = profile();
        if !linked {
            p.managed_source = Some(owner());
        }
        let f = fixture(
            p,
            if linked {
                vec![subscription(true)]
            } else {
                vec![]
            },
        );
        for explicit in [false, true] {
            let s = snapshot(&f.state, "manual", explicit).unwrap();
            assert!(s.binding.is_none());
            assert!(s.rows.is_empty());
            assert!(s.targets.is_empty());
            assert!(s.read_only_reason.is_some());
            assert!(!serde_json::to_string(&s).unwrap().contains("secret-canary"));
        }
        assert!(preview(
            &f.state,
            PreviewInput {
                profile_id: "manual".into(),
                binding: "fake".into(),
                mode: Mode::Replace,
                rows: vec![]
            }
        )
        .is_err());
    }
}
#[test]
fn ordinary_subscription_accepts_overlay_without_writing_subscription_or_source() {
    let f = fixture(profile(), vec![subscription(false)]);
    let before = workspace::digest(
        &app_database::load_domain_data(f.dir.path())
            .unwrap()
            .subscriptions,
    )
    .unwrap();
    let p = prepare(&f);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    assert_eq!(
        before,
        workspace::digest(
            &app_database::load_domain_data(f.dir.path())
                .unwrap()
                .subscriptions
        )
        .unwrap()
    );
}
#[test]
fn file_and_geoip_dependency_guard_never_reads_fifo_or_returns_material() {
    for geoip in [false, true] {
        let mut p = profile();
        let mut raw = source();
        if geoip {
            raw["route"]["geoip_database"] = json!("/untrusted/secret-canary.fifo");
        } else {
            raw["route"]["rule_sets"] = json!([{"tag":"x","type":"file","path":"/untrusted/secret-canary.fifo","format":"zrs"}]);
        }
        p.content = Some(raw);
        let f = fixture(p, vec![]);
        let s = snapshot(&f.state, "manual", true).unwrap();
        assert!(s.read_only_reason.is_some());
        assert!(s.rows.is_empty());
        assert!(!serde_json::to_string(&s).unwrap().contains("secret-canary"));
    }
}
#[test]
fn source_same_timestamp_changes_and_persistent_relationship_aba_reject_original_plan() {
    let f = fixture(profile(), vec![]);
    let p = prepare(&f);
    let original = lock(f.state.proxy_configs(), "p").unwrap().clone();
    let mut altered = original.clone();
    altered[0].name = "changed-same-ms".into();
    app_database::save_domain_data(f.dir.path(), &altered, &[]).unwrap();
    app_database::save_domain_data(f.dir.path(), &original, &[]).unwrap();
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    assert!(overlay(&lock(f.state.app_config(), "a").unwrap(), "manual").is_none());
    let f = fixture(profile(), vec![]);
    let p = prepare(&f);
    lock(f.state.proxy_configs(), "p").unwrap()[0]
        .content
        .as_mut()
        .unwrap()["route"]["final"] = json!({"type":"reject"});
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
}
#[test]
fn app_and_rules_file_aba_reject_even_when_current_values_match_original() {
    for app_domain in [false, true] {
        let f = fixture(profile(), vec![]);
        let p = prepare(&f);
        if app_domain {
            let before = lock(f.state.app_config(), "a").unwrap().clone();
            let mut changed = before.clone();
            changed.mobile_notifications_enabled = !changed.mobile_notifications_enabled;
            app_config_store::save(&f.dir.path().join("app-config.json"), &changed).unwrap();
            app_config_store::save(&f.dir.path().join("app-config.json"), &before).unwrap();
        } else {
            let path = f.dir.path().join("rule-sets.json");
            let mut guard = file_history::acquire(&path, Domain::RuleSets).unwrap();
            guard
                .mutate(b"[] ", None, None, || {
                    std::fs::write(&path, b"[] ").map_err(|_| storage())
                })
                .unwrap();
            drop(guard);
            domain_store::save_rule_sets_to_dir(f.dir.path(), &[]).unwrap();
        }
        assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    }
}
#[test]
fn instance_generation_aba_and_expiration_are_rejected_before_file_write() {
    let f = fixture(profile(), vec![]);
    let p = prepare(&f);
    let original = lock(f.state.proxy_configs(), "p").unwrap()[0].clone();
    f.state.client_core_configuration_committed(None);
    f.state.client_core_configuration_committed(Some(&original));
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    let p = prepare(&f);
    lock(&f.state.route_editor().plans, "plans")
        .unwrap()
        .get_mut(&p.plan_id)
        .unwrap()
        .deadline = Instant::now() - Duration::from_secs(1);
    assert!(save(&f.state, &p.plan_id).is_err());
}
#[test]
fn uncertain_file_delivery_blocks_other_prepared_plans_new_plans_and_discard_without_replay() {
    let f = fixture(profile(), vec![]);
    let a = prepare(&f);
    let b = prepare(&f);
    let third = draft(&f);
    f.state
        .route_editor()
        .fail_after_write
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        save(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    let receipt = app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert!(save(&f.state, &b.plan_id).is_err());
    assert!(preview(&f.state, third).is_err());
    assert!(discard(&f.state, &a.plan_id).is_err());
    assert_eq!(
        receipt,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
    assert!(overlay(&lock(f.state.app_config(), "a").unwrap(), "manual").is_none());
}
#[test]
fn stale_overlay_requires_explicit_current_source_and_restore_differs_from_empty_replace() {
    let f = fixture(profile(), vec![]);
    let mut input = draft(&f);
    input.rows.clear();
    let p = preview(&f.state, input).unwrap();
    save(&f.state, &p.plan_id).unwrap();
    assert!(snapshot(&f.state, "manual", false).unwrap().rows.is_empty());
    let before_source = f.state.client_core_snapshot();
    let mut profiles = lock(f.state.proxy_configs(), "p").unwrap();
    profiles[0].content.as_mut().unwrap()["route"]["final"] = json!({"type":"reject"});
    let changed = profiles.clone();
    drop(profiles);
    app_database::save_domain_data(f.dir.path(), &changed, &[]).unwrap();
    // Simulate the complete ordinary source publisher: durable/memory source
    // publication is followed by invalidation, exactly as production writers do.
    f.state
        .client_core_configuration_committed(Some(&changed[0]));
    let after_source = f.state.client_core_snapshot();
    assert_ne!(
        after_source.scope.config_revision,
        before_source.scope.config_revision
    );
    assert_eq!(
        after_source.scope_generation.0,
        before_source.scope_generation.0 + 1
    );
    let stale = snapshot(&f.state, "manual", false).unwrap();
    assert!(stale.binding.is_none());
    assert!(stale.stale_reason.is_some());
    assert!(stale.rows.is_empty());
    let fresh = snapshot(&f.state, "manual", true).unwrap();
    assert!(fresh.binding.is_some());
    assert_eq!(fresh.rows.len(), 1);
    let p = preview(
        &f.state,
        PreviewInput {
            profile_id: "manual".into(),
            binding: fresh.binding.unwrap(),
            mode: Mode::RestoreSource,
            rows: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    assert!(
        !snapshot(&f.state, "manual", false)
            .unwrap()
            .has_local_overlay
    );
    assert_eq!(snapshot(&f.state, "manual", false).unwrap().rows.len(), 1);
}

#[test]
fn incoherent_source_cache_cannot_be_confirmed_by_the_save_own_invalidation() {
    let f = fixture(profile(), vec![]);
    let original_scope = f.state.client_core_snapshot();
    let mut profiles = lock(f.state.proxy_configs(), "p").unwrap();
    profiles[0].content.as_mut().unwrap()["route"]["final"] = json!({"type":"reject"});
    let changed = profiles.clone();
    drop(profiles);
    app_database::save_domain_data(f.dir.path(), &changed, &[]).unwrap();
    // Deliberately omit the source writer's invalidation. A subsequent save's
    // own notification must not silently grant this stale scope a receipt.
    let p = prepare(&f);
    let result = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(result.state, "submitted_unknown");
    assert!(result.snapshot.is_none());
    assert_ne!(
        f.state.client_core_snapshot().scope.config_revision,
        original_scope.scope.config_revision
    );
    assert_eq!(
        status(&f.state, &p.plan_id).unwrap().state,
        "submitted_unknown"
    );
}
#[test]
fn duplicate_ids_disabled_invalid_references_and_native_duplicate_fields_are_not_accepted() {
    for kind in 0..4 {
        let f = fixture(profile(), vec![]);
        let mut input = draft(&f);
        match kind {0=>input.rows.push(input.rows[0].clone()),1=>{input.rows[0].enabled=false;input.rows[0].rule_json=json!({"condition":{"type":"domain","values":["example.com"]},"action":{"type":"route","outbound":"secret-canary"}}).to_string();},2=>input.rows[0].rule_json=r#"{"condition":{"type":"domain","values":["example.com"]},"action":{"type":"direct"},"action":{"type":"reject"}}"#.into(),_=>input.rows[0].rule_json=r#"{"condition":{"type":"secret-canary"},"action":{"type":"direct"}}"#.into()};
        let e = preview(&f.state, input).unwrap_err();
        assert!(!serde_json::to_string(&e).unwrap().contains("secret-canary"));
    }
}

#[test]
fn preview_preserves_exact_original_text_and_native_endpoint_targets() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let mut p = profile();
    p.content.as_mut().unwrap()["endpoints"] = json!([{"tag":"canonical-device","protocol":{
        "type":"wireguard","private_key":STANDARD.encode([1;32]),"addresses":["10.0.0.1/32"],
        "peers":[{"public_key":STANDARD.encode([2;32]),"endpoint":"127.0.0.1:51821","allowed_ips":["10.0.0.0/24"]}]}}]);
    let f = fixture(p, vec![]);
    let s = snapshot(&f.state, "manual", false).unwrap();
    assert!(s
        .targets
        .iter()
        .any(|t| t.tag.as_deref() == Some("canonical-device")));
    let mut input = draft(&f);
    let text = "{\"condition\":{\"type\":\"domain\",\"values\":[\"example.com\"]}, \"action\":{\"type\":\"route\",\"outbound\":\"canonical-device\"},\"mode\":\"flow\"}";
    input.rows[0].rule_json = text.into();
    let p = preview(&f.state, input).unwrap();
    assert_eq!(p.rows[0].rule_json, text);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
}

#[test]
fn unknown_existing_client_edits_do_not_escape_safe_preview_error() {
    let f = fixture(profile(), vec![]);
    let mut app = lock(f.state.app_config(), "a").unwrap();
    app.client_edits
        .as_mut()
        .unwrap()
        .insert("secret-canary-key".into(), json!("secret-canary-value"));
    app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
    drop(app);
    let error = preview(&f.state, draft(&f)).unwrap_err();
    assert_eq!(error.code, "route_editor_validation");
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("secret-canary"));
}

#[test]
fn subsequent_actual_file_writer_is_not_returned_as_original_save_receipt() {
    fn overwrite(state: &AppState) {
        let dir = workspace::recovery::directory(state).unwrap();
        let mut app = lock(state.app_config(), "a").unwrap();
        app.profile_edits
            .get_mut("manual")
            .unwrap()
            .get_mut("routeRules")
            .unwrap()["rows"] = json!([]);
        app_config_store::save(&dir.join("app-config.json"), &app).unwrap();
    }
    let f = fixture(profile(), vec![]);
    let p = prepare(&f);
    *lock(&f.state.route_editor().after_publish, "hook").unwrap() = Some(overwrite);
    let result = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(result.state, "submitted_unknown");
    assert!(result.snapshot.is_none());
    let current = app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert_eq!(
        current,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
}
