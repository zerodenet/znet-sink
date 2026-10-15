use super::*;
use crate::{
    models::{proxy_config::ProxyConfigCapabilities, subscription::ManagedSubscriptionSource},
    services::{app_config_store, app_database, domain_store},
};

fn dns() -> Value {
    json!({"servers":{"old":{"type":"system"},"new":{"type":"system"},"backup":{"type":"system"}},
        "default_server":"old","dispatch":[{"condition":{"type":"domain","values":["old"]},"server":"old"}],
        "cache":{"max_entries":8,"max_ttl_seconds":u64::MAX},
        "answer":{"type":"real"},"policy":{"node_server":"old","direct_server":"old","fallback_servers":["backup"],"node_fallback_servers":["backup"],"direct_fallback_servers":["backup"],"server_timeout_ms":{"old":5000}}})
}
fn source() -> Value {
    json!({"route":{"rules":[],"final":{"type":"direct"}},"runtime":{"dns":dns(),"udp_upstream_idle_timeout_seconds":u64::MAX}})
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
struct Fixture {
    state: AppState,
    dir: tempfile::TempDir,
}
fn fixture(profiles: Vec<ProxyConfigProfile>, subs: Vec<SubscriptionProfile>) -> Fixture {
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
    app_database::save_domain_data(dir.path(), &profiles, &subs).unwrap();
    app_config_store::save(&dir.path().join("app-config.json"), &app).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    let state = AppState::with_domain_data(app, profiles, subs, vec![], vec![]);
    workspace::recovery::set_test_directory(&state, dir.path().to_path_buf());
    Fixture { state, dir }
}
fn input(f: &Fixture, id: &str) -> PreviewInput {
    let s = snapshot(&f.state, id).unwrap();
    PreviewInput {
        profile_id: id.into(),
        binding: s.binding.unwrap(),
        server_name: "old".into(),
        replacement_name: Some("new".into()),
    }
}
fn prepare(f: &Fixture) -> Plan {
    preview(&f.state, input(f, "manual")).unwrap()
}
fn persisted(f: &Fixture) -> AppConfig {
    app_config_store::load_or_default(&f.dir.path().join("app-config.json")).unwrap()
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
    let mut s:SubscriptionProfile=serde_json::from_value(json!({"id":"sub","name":"sub","url":"https://example.invalid/?token=secret-canary","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":"manual","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap();
    if managed {
        s.managed_source = Some(owner());
    }
    s
}

#[test]
fn actual_app_file_reopen_migrates_all_primary_dispatch_timeout_references_and_never_changes_source_or_runtime(
) {
    let f = fixture(vec![profile()], vec![]);
    let before = app_database::load_domain_data(f.dir.path()).unwrap();
    let epoch = workspace_recovery::current_epoch(f.dir.path()).unwrap();
    #[cfg(unix)]
    let listener =
        std::os::unix::net::UnixListener::bind(f.dir.path().join("no-runtime.sock")).unwrap();
    #[cfg(unix)]
    listener.set_nonblocking(true).unwrap();
    let p = prepare(&f);
    assert_eq!(p.references.len(), 5);
    assert!(p.notice.contains("影响所有配置"));
    let result = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(result.state, "saved_pending_apply");
    let app = persisted(&f);
    let config = &app.client_edits.as_ref().unwrap()["dns"]["config"];
    assert!(config["servers"].get("old").is_none());
    assert_eq!(config["default_server"], "new");
    assert_eq!(config["dispatch"][0]["server"], "new");
    assert_eq!(config["dispatch"][0]["condition"]["values"][0], "old");
    assert_eq!(config["policy"]["node_server"], "new");
    assert_eq!(config["policy"]["direct_server"], "new");
    assert_eq!(config["policy"]["server_timeout_ms"]["new"], 5000);
    assert_eq!(config["cache"]["max_ttl_seconds"].as_u64(), Some(u64::MAX));
    assert_eq!(
        app.local_proxy.source_proxy_config_id.as_deref(),
        Some("runtime-owned")
    );
    assert_eq!(
        workspace::digest(&before.proxy_configs).unwrap(),
        workspace::digest(
            &app_database::load_domain_data(f.dir.path())
                .unwrap()
                .proxy_configs
        )
        .unwrap()
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
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    assert_eq!(
        history,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
}
#[test]
fn all_three_ordered_fallback_chains_migrate_without_reordering() {
    for (field, primary) in [
        ("fallback_servers", None),
        ("node_fallback_servers", Some("node_server")),
        ("direct_fallback_servers", Some("direct_server")),
    ] {
        let mut original = dns();
        original["default_server"] = json!("backup");
        original["dispatch"] = json!([]);
        original["policy"] = json!({});
        if let Some(primary) = primary {
            original["policy"][primary] = json!("backup");
        }
        original["policy"][field] = json!(["old"]);
        let (next, refs) = transforms::migrate(&original, "old", Some("new")).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(next["policy"][field], json!(["new"]));
        assert_eq!(original["policy"][field], json!(["old"]));
    }
}
#[test]
fn replacement_must_be_explicit_existing_and_different_and_final_server_cannot_be_deleted() {
    for replacement in [None, Some("missing"), Some("old")] {
        assert!(transforms::migrate(&dns(), "old", replacement).is_err());
    }
    let mut one = dns();
    one["servers"] = json!({"old":{"type":"system"}});
    assert!(transforms::migrate(&one, "old", None).is_err());
    let (next, refs) = transforms::migrate(&dns(), "backup", Some("new")).unwrap();
    assert_eq!(refs.len(), 3);
    assert_eq!(next["policy"]["fallback_servers"], json!(["new"]));
    let mut unused = dns();
    unused["servers"]["unused"] = json!({"type":"system"});
    let (next, refs) = transforms::migrate(&unused, "unused", None).unwrap();
    assert!(refs.is_empty());
    assert!(next["servers"].get("unused").is_none());
    assert_eq!(next["default_server"], "old");
}
#[test]
fn duplicate_fallback_primary_collision_and_timeout_collision_are_rejected_without_mutation() {
    for conflict in 0..3 {
        let mut original = dns();
        match conflict {
            0 => original["policy"]["fallback_servers"] = json!(["backup", "new"]),
            1 => {
                original["default_server"] = json!("backup");
                original["policy"]["fallback_servers"] = json!(["old"]);
            }
            _ => original["policy"]["server_timeout_ms"]["new"] = json!(5000),
        };
        let before = original.clone();
        let target = if conflict == 0 { "backup" } else { "old" };
        assert!(transforms::migrate(
            &original,
            target,
            Some(if conflict == 1 { "backup" } else { "new" })
        )
        .is_err());
        assert_eq!(original, before);
    }
}
#[test]
fn opaque_matching_extra_is_ambiguous_and_unrelated_unrecognized_schema_is_not_stripped() {
    for field in ["future", "policy", "dispatch", "server"] {
        let mut original = dns();
        match field {
            "policy" => original["policy"]["future"] = json!({"nested":["old"]}),
            "dispatch" => original["dispatch"][0]["future"] = json!("old"),
            "server" => original["servers"]["backup"]["future"] = json!({"old":u64::MAX}),
            _ => original["future"] = json!("old"),
        };
        let e = transforms::migrate(&original, "old", Some("new")).unwrap_err();
        assert_eq!(e.code, "dns_migration_ambiguous");
        assert!(original.to_string().contains("old"));
    }
    let mut original = dns();
    original["future"] = json!(u64::MAX);
    let before = original.clone();
    assert_eq!(
        transforms::migrate(&original, "old", Some("new"))
            .unwrap_err()
            .code,
        "dns_migration_validation"
    );
    assert_eq!(original, before);
}
#[test]
fn reference_paths_escape_timeout_server_names_and_do_not_replace_domains_hosts_or_outbound_detours(
) {
    let mut original = dns();
    original["servers"].as_object_mut().unwrap().remove("old");
    original["servers"]["old/~"] = json!({"type":"system"});
    original["default_server"] = json!("old/~");
    original["dispatch"] = json!([]);
    original["policy"] = json!({"server_timeout_ms":{"old/~":6000}});
    let (next, refs) = transforms::migrate(&original, "old/~", Some("new")).unwrap();
    assert!(refs
        .iter()
        .any(|r| r.path == "/policy/server_timeout_ms/old~1~0"));
    assert_eq!(next["policy"]["server_timeout_ms"]["new"], 6000);
    let mut original = dns();
    original["servers"]["backup"] =
        json!({"type":"udp","host":"old","bootstrap":[],"detour":"old"});
    original["policy"]["node_fallback_servers"] = json!([]);
    original["policy"]["direct_fallback_servers"] = json!([]);
    original["policy"]["fallback_servers"] = json!([]);
    // A detoured backend cannot itself be a node resolver; the node remains
    // the system backend old/new and endpoint strings must remain untouched.
    let (next, _) = transforms::migrate(&original, "old", Some("new")).unwrap();
    assert_eq!(next["servers"]["backup"]["host"], "old");
    assert_eq!(next["servers"]["backup"]["detour"], "old");
}
#[test]
fn disabled_saved_override_is_validated_and_preserves_intent_and_u64() {
    let f = fixture(vec![profile()], vec![]);
    let mut app = lock(f.state.app_config(), "a").unwrap();
    app.client_edits.as_mut().unwrap().insert(
        "dns".into(),
        json!({"enabled":false,"dnsHijack":false,"config":dns()}),
    );
    app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
    drop(app);
    let p = prepare(&f);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    let app = persisted(&f);
    let wrapper = &app.client_edits.as_ref().unwrap()["dns"];
    assert_eq!(wrapper["enabled"], false);
    assert_eq!(wrapper["dnsHijack"], false);
    assert_eq!(
        wrapper["config"]["cache"]["max_ttl_seconds"].as_u64(),
        Some(u64::MAX)
    );
    let mut invalid = dns();
    invalid["policy"]["node_server"] = json!("missing-secret-canary");
    let e = transforms::migrate(&invalid, "old", Some("new")).unwrap_err();
    assert!(!serde_json::to_string(&e).unwrap().contains("secret-canary"));
}
#[test]
fn managed_profile_managed_relation_and_missing_body_do_not_expose_dns_names() {
    for kind in 0..3 {
        let mut p = profile();
        match kind {
            0 => p.managed_source = Some(owner()),
            1 => {}
            _ => p.content = None,
        };
        let f = fixture(
            vec![p],
            if kind == 1 {
                vec![subscription(true)]
            } else {
                vec![]
            },
        );
        let s = snapshot(&f.state, "manual").unwrap();
        assert!(s.binding.is_none());
        assert!(s.servers.is_empty());
        assert!(s.read_only_reason.is_some());
        assert!(!serde_json::to_string(&s).unwrap().contains("secret-canary"));
        assert!(preview(
            &f.state,
            PreviewInput {
                profile_id: "manual".into(),
                binding: "fake".into(),
                server_name: "old".into(),
                replacement_name: Some("new".into())
            }
        )
        .is_err());
    }
}
#[test]
fn ordinary_subscription_source_and_unread_rule_geoip_paths_remain_unchanged() {
    let mut p = profile();
    let source = p.content.as_mut().unwrap();
    source["route"]["rule_sets"] = json!([{"tag":"external","type":"file","path":"/unread/secret-canary.fifo","format":"zrs"}]);
    source["route"]["geoip_database"] = json!("/unread/secret-canary.mmdb");
    let f = fixture(vec![p], vec![subscription(false)]);
    let before = app_database::load_domain_data(f.dir.path()).unwrap();
    let p = prepare(&f);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    let after = app_database::load_domain_data(f.dir.path()).unwrap();
    assert_eq!(
        workspace::digest(&before.proxy_configs).unwrap(),
        workspace::digest(&after.proxy_configs).unwrap()
    );
    assert_eq!(
        workspace::digest(&before.subscriptions).unwrap(),
        workspace::digest(&after.subscriptions).unwrap()
    );
}
#[test]
fn absent_dns_returns_explanatory_readonly_snapshot() {
    let mut p = profile();
    p.content.as_mut().unwrap()["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("dns");
    let f = fixture(vec![p], vec![]);
    // AppConfig::default intentionally supplies a recommended DNS fallback;
    // remove that saved fallback too to exercise a genuinely absent config.
    let mut app = lock(f.state.app_config(), "a").unwrap();
    app.dns = crate::models::app_config::AppDnsConfig::default();
    app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
    drop(app);
    let s = snapshot(&f.state, "manual").unwrap();
    assert!(s.binding.is_none());
    assert!(s.servers.is_empty());
    assert!(s.read_only_reason.is_some());
}

#[test]
fn missing_source_dns_uses_actual_saved_default_fallback_without_inventing_empty_state() {
    let mut p = profile();
    p.content.as_mut().unwrap()["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("dns");
    let f = fixture(vec![p], vec![]);
    let s = snapshot(&f.state, "manual").unwrap();
    assert!(s.binding.is_some());
    assert!(s.read_only_reason.is_none());
    let default_names = lock(f.state.app_config(), "a")
        .unwrap()
        .dns
        .config
        .as_ref()
        .unwrap()
        .servers
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        s.servers.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        default_names
    );
}
#[test]
fn relationship_and_app_history_aba_reject_fixed_plan_without_overwriting_files() {
    for app_domain in [false, true] {
        let f = fixture(vec![profile()], vec![]);
        let p = prepare(&f);
        if app_domain {
            let before = lock(f.state.app_config(), "a").unwrap().clone();
            let mut changed = before.clone();
            changed.mobile_notifications_enabled = true;
            app_config_store::save(&f.dir.path().join("app-config.json"), &changed).unwrap();
            app_config_store::save(&f.dir.path().join("app-config.json"), &before).unwrap();
        } else {
            let original = lock(f.state.proxy_configs(), "p").unwrap().clone();
            let mut changed = original.clone();
            changed[0].name = "same-ms-other".into();
            app_database::save_domain_data(f.dir.path(), &changed, &[]).unwrap();
            app_database::save_domain_data(f.dir.path(), &original, &[]).unwrap();
        }
        assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
        assert!(!persisted(&f)
            .client_edits
            .as_ref()
            .unwrap()
            .contains_key("dns"));
    }
}
#[test]
fn expiration_scope_aba_and_midplan_ownership_changes_are_rejected() {
    for mode in 0..3 {
        let f = fixture(vec![profile()], vec![]);
        let p = prepare(&f);
        match mode {
            0 => {
                let original = lock(f.state.proxy_configs(), "p").unwrap()[0].clone();
                f.state.client_core_configuration_committed(None);
                f.state.client_core_configuration_committed(Some(&original));
            }
            1 => {
                lock(f.state.proxy_configs(), "p").unwrap()[0].managed_source = Some(owner());
            }
            _ => {
                lock(&f.state.dns_migration().plans, "plans")
                    .unwrap()
                    .get_mut(&p.plan_id)
                    .unwrap()
                    .deadline = Instant::now() - Duration::from_secs(1);
            }
        }
        if mode == 2 {
            assert!(save(&f.state, &p.plan_id).is_err());
        } else {
            assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
        }
        assert!(!persisted(&f)
            .client_edits
            .as_ref()
            .unwrap()
            .contains_key("dns"));
    }
}
#[test]
fn unknown_global_dns_write_blocks_preexisting_and_new_other_profile_plans_without_replay() {
    let mut other = profile();
    other.id = "other".into();
    other.active = false;
    let f = fixture(vec![profile(), other], vec![]);
    let a = prepare(&f);
    let b = preview(&f.state, input(&f, "other")).unwrap();
    let third = input(&f, "other");
    f.state
        .dns_migration()
        .fail_after_write
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        save(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    let history = app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert!(save(&f.state, &b.plan_id).is_err());
    assert!(preview(&f.state, third).is_err());
    assert!(discard(&f.state, &a.plan_id).is_err());
    assert_eq!(
        history,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
}
#[test]
fn subsequent_file_writer_is_not_accepted_as_original_receipt() {
    fn change(state: &AppState) {
        let dir = workspace::recovery::directory(state).unwrap();
        let mut app = lock(state.app_config(), "a").unwrap();
        app.mobile_notifications_enabled = true;
        app_config_store::save(&dir.join("app-config.json"), &app).unwrap();
    }
    let f = fixture(vec![profile()], vec![]);
    let p = prepare(&f);
    *lock(&f.state.dns_migration().after_publish, "hook").unwrap() = Some(change);
    let result = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(result.state, "submitted_unknown");
    assert!(result.snapshot.is_none());
}

#[test]
fn legacy_shortcuts_reject_deleted_or_renamed_servers_even_when_disabled_before_any_local_or_runtime_effect(
) {
    for rename in [false, true] {
        let f = fixture(vec![profile()], vec![]);
        #[cfg(unix)]
        let listener =
            std::os::unix::net::UnixListener::bind(f.dir.path().join("no-runtime.sock")).unwrap();
        #[cfg(unix)]
        listener.set_nonblocking(true).unwrap();
        let old = lock(f.state.app_config(), "a").unwrap().clone();
        let before = std::fs::read(f.dir.path().join("app-config.json")).unwrap();
        let history =
            app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
        let graph = workspace_recovery::current_epoch(f.dir.path()).unwrap();
        let scope = f.state.client_core_snapshot();
        let mut config = transforms::migrate(&dns(), "old", Some("new")).unwrap().0;
        if rename {
            config["servers"]["renamed"] = json!({"type":"system"});
        }
        let mut next = old.clone();
        next.client_edits.as_mut().unwrap().insert(
            "dns".into(),
            json!({"enabled":false,"dnsHijack":false,"config":config}),
        );
        let error = require_legacy_server_preserved(&f.state, &old, &next).unwrap_err();
        assert_eq!(error.code, "dns_migration_preview_required");
        assert!(error.details.is_none());
        assert_eq!(
            before,
            std::fs::read(f.dir.path().join("app-config.json")).unwrap()
        );
        assert_eq!(
            history,
            app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
        );
        assert_eq!(
            graph,
            workspace_recovery::current_epoch(f.dir.path()).unwrap()
        );
        assert_eq!(
            scope.scope_generation,
            f.state.client_core_snapshot().scope_generation
        );
        assert_eq!(*lock(f.state.app_config(), "a").unwrap(), old);
        assert!(f.state.configuration().transaction().is_none());
        #[cfg(unix)]
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn legacy_additions_parameter_changes_and_disabled_existing_material_remain_allowed() {
    let f = fixture(vec![profile()], vec![]);
    let old = lock(f.state.app_config(), "a").unwrap().clone();
    for kind in 0..3 {
        let mut config = dns();
        match kind {
            0 => config["servers"]["added"] = json!({"type":"system"}),
            1 => config["policy"]["timeout_ms"] = json!(6000),
            _ => {}
        }
        let mut next = old.clone();
        next.client_edits.as_mut().unwrap().insert(
            "dns".into(),
            json!({"enabled":kind!=2,"dnsHijack":false,"config":config}),
        );
        assert!(require_legacy_server_preserved(&f.state, &old, &next).is_ok());
    }
}

#[test]
fn legacy_guard_compares_explicit_overrides_and_real_default_fallback_and_preserves_protected_admission(
) {
    let mut p = profile();
    p.content.as_mut().unwrap()["runtime"]
        .as_object_mut()
        .unwrap()
        .remove("dns");
    let f = fixture(vec![p], vec![]);
    let old = lock(f.state.app_config(), "a").unwrap().clone();
    let mut config = serde_json::to_value(old.dns.config.as_ref().unwrap()).unwrap();
    let first = config["servers"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    config["servers"].as_object_mut().unwrap().remove(&first);
    let mut next = old.clone();
    next.client_edits.as_mut().unwrap().insert(
        "dns".into(),
        json!({"enabled":false,"dnsHijack":false,"config":config}),
    );
    assert_eq!(
        require_legacy_server_preserved(&f.state, &old, &next)
            .unwrap_err()
            .code,
        "dns_migration_preview_required"
    );
    let f = fixture(vec![profile()], vec![]);
    let mut old = lock(f.state.app_config(), "a").unwrap().clone();
    old.client_edits.as_mut().unwrap().insert(
        "dns".into(),
        json!({"enabled":false,"dnsHijack":false,"config":dns()}),
    );
    let mut next = old.clone();
    next.client_edits.as_mut().unwrap().get_mut("dns").unwrap()["config"]["servers"]
        .as_object_mut()
        .unwrap()
        .remove("old");
    assert_eq!(
        require_legacy_server_preserved(&f.state, &old, &next)
            .unwrap_err()
            .code,
        "dns_migration_preview_required"
    );
    lock(f.state.proxy_configs(), "p").unwrap()[0].managed_source = Some(owner());
    let mut next = old.clone();
    next.client_edits.as_mut().unwrap().remove("dns");
    let error = require_legacy_server_preserved(&f.state, &old, &next).unwrap_err();
    assert_eq!(error.code, "dns_migration_read_only");
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("secret-canary"));
}
