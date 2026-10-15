use super::*;
use crate::{
    models::{
        app_config::AppConfig, proxy_config::ProxyConfigCapabilities,
        subscription::ManagedSubscriptionSource,
    },
    services::{app_config_store, app_database, domain_store},
};
use serde_json::{json, Value};

fn source() -> Value {
    json!({"route":{"rules":[],"final":{"type":"direct"},"final_mode":"auto"},"runtime":{"udp_upstream_idle_timeout_seconds":u64::MAX}})
}
fn profile(active: bool) -> ProxyConfigProfile {
    ProxyConfigProfile {
        id: "manual".into(),
        name: "original".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(source()),
        active,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: ProxyConfigCapabilities::default(),
    }
}
fn subscription(managed: bool) -> SubscriptionProfile {
    let mut s: SubscriptionProfile = serde_json::from_value(json!({"id":"sub","name":"sub","url":"https://example.invalid/?token=secret-canary","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":"manual","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap();
    if managed {
        s.managed_source = Some(owner());
    }
    s
}
fn owner() -> ManagedSubscriptionSource {
    ManagedSubscriptionSource {
        plugin_id: "publisher.plugin".into(),
        provider_id: "source".into(),
        remote_subscription_id: "remote".into(),
        revision: None,
        source_name: None,
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
fn draft(snapshot: &Snapshot, text: &str) -> DraftInput {
    DraftInput {
        id: snapshot.target_id.clone(),
        binding: snapshot.binding.clone().unwrap(),
        source_json: text.into(),
    }
}
fn input(snapshot: &Snapshot, name: &str, text: &str) -> SaveInput {
    SaveInput {
        id: snapshot.target_id.clone(),
        binding: snapshot.binding.clone().unwrap(),
        name: name.into(),
        source_json: text.into(),
    }
}
fn current(f: &Fixture) -> Vec<ProxyConfigProfile> {
    app_database::load_domain_data(f.dir.path())
        .unwrap()
        .proxy_configs
}

#[test]
fn new_profile_is_inactive_and_roundtrips_native_u64_and_omitted_fields_through_sqlite_reopen() {
    let f = fixture(vec![], vec![]);
    let before = snapshot(&f.state, None).unwrap();
    assert!(before.target_id.is_none());
    assert!(before.source_json.is_none());
    assert_eq!(
        before.create_target_id,
        snapshot(&f.state, None).unwrap().create_target_id
    );
    let raw = source().to_string();
    let pretty = validate(&f.state, draft(&before, &raw)).unwrap();
    assert!(pretty.source_json.contains("18446744073709551615"));
    let result = save(&f.state, input(&before, "new", &pretty.source_json)).unwrap();
    assert_eq!(result.snapshot.target_id, before.create_target_id);
    assert!(result.snapshot.create_target_id.is_none());
    assert_eq!(result.state, "saved_pending_apply");
    assert!(result
        .snapshot
        .source_json
        .as_ref()
        .unwrap()
        .contains("18446744073709551615"));
    let loaded = current(&f);
    assert_eq!(loaded.len(), 1);
    assert!(!loaded[0].active);
    assert_eq!(loaded[0].content, Some(source()));
    assert!(loaded[0]
        .content
        .as_ref()
        .unwrap()
        .get("inbounds")
        .is_none());
    let app = app_config_store::load_or_default(&f.dir.path().join("app-config.json")).unwrap();
    let reloaded = AppState::with_domain_data(app, loaded, vec![], vec![], vec![]);
    workspace::recovery::set_test_directory(&reloaded, f.dir.path().to_path_buf());
    let next = snapshot(&reloaded, result.snapshot.target_id.as_deref()).unwrap();
    assert_eq!(next.source_json, result.snapshot.source_json);
    assert!(f.state.configuration().transaction().is_none());
}
#[test]
fn active_local_edit_retains_selection_and_runtime_proxy_owner_without_rewriting_settings() {
    let f = fixture(vec![profile(true)], vec![]);
    let path = f.dir.path().join("app-config.json");
    let bytes = std::fs::read(&path).unwrap();
    let history = app_config_store::current_history(&path).unwrap();
    let before = snapshot(&f.state, Some("manual")).unwrap();
    let generation = f.state.client_core_snapshot().scope_generation;
    save(&f.state, input(&before, "renamed", &source().to_string())).unwrap();
    assert!(current(&f)[0].active);
    assert!(f.state.client_core_snapshot().scope_generation > generation);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(app_config_store::current_history(&path).unwrap(), history);
    assert_eq!(
        f.state
            .app_config()
            .lock()
            .unwrap()
            .local_proxy
            .source_proxy_config_id
            .as_deref(),
        Some("runtime-owned")
    );
    assert!(f.state.configuration().transaction().is_none());
}
#[cfg(unix)]
#[test]
fn local_save_does_not_connect_to_an_actual_available_control_endpoint() {
    use std::os::unix::net::UnixListener;
    let f = fixture(vec![profile(true)], vec![]);
    let listener = UnixListener::bind(f.dir.path().join("no-runtime.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    f.state.client_core_instance_started();
    let before = snapshot(&f.state, Some("manual")).unwrap();
    save(&f.state, input(&before, "local", &source().to_string())).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(f.state.configuration().transaction().is_none());
}
#[test]
fn protected_managed_path_only_and_managed_relation_are_denied_before_body_or_file_access() {
    for case in 0..4 {
        let mut p = profile(false);
        p.content = Some(json!({"secret-canary":"must-not-appear"}));
        let mut subs = vec![];
        match case {
            0 => p.managed_source = Some(owner()),
            1 => p.content = None,
            2 => {
                p.content = None;
                p.path = Some("/never-read-secret-canary".into())
            }
            _ => subs.push(subscription(true)),
        }
        let f = fixture(vec![p], subs);
        let view = snapshot(&f.state, Some("manual")).unwrap();
        assert!(view.source_json.is_none());
        assert!(view.binding.is_none());
        assert!(view.read_only_reason.is_some());
        assert!(!serde_json::to_string(&view)
            .unwrap()
            .contains("secret-canary"));
        let d = DraftInput {
            id: Some("manual".into()),
            binding: "forged".into(),
            source_json: "secret-canary".into(),
        };
        assert_eq!(
            validate(&f.state, d).unwrap_err().code,
            "profile_editor_read_only"
        );
        let s = SaveInput {
            id: Some("manual".into()),
            binding: "forged".into(),
            name: "x".into(),
            source_json: "secret-canary".into(),
        };
        assert_eq!(
            save(&f.state, s).unwrap_err().code,
            "profile_editor_read_only"
        );
    }
}
#[test]
fn ordinary_subscription_body_is_visible_but_cannot_be_rewritten_or_detached() {
    let f = fixture(vec![profile(false)], vec![subscription(false)]);
    let view = snapshot(&f.state, Some("manual")).unwrap();
    assert!(view.source_json.unwrap().contains("18446744073709551615"));
    assert!(view.binding.is_none());
    let d = SaveInput {
        id: Some("manual".into()),
        binding: "forged".into(),
        name: "x".into(),
        source_json: source().to_string(),
    };
    assert_eq!(
        save(&f.state, d).unwrap_err().code,
        "profile_editor_read_only"
    );
    let data = app_database::load_domain_data(f.dir.path()).unwrap();
    assert_eq!(
        data.subscriptions[0].target_proxy_config_id.as_deref(),
        Some("manual")
    );
    assert_eq!(data.proxy_configs[0].name, "original");
}
#[test]
fn readonly_transition_after_snapshot_blocks_original_validate_and_save() {
    let f = fixture(vec![profile(false)], vec![]);
    let before = snapshot(&f.state, Some("manual")).unwrap();
    f.state.proxy_configs().lock().unwrap()[0].managed_source = Some(owner());
    assert_eq!(
        validate(&f.state, draft(&before, &source().to_string()))
            .unwrap_err()
            .code,
        "profile_editor_read_only"
    );
    assert_eq!(
        save(&f.state, input(&before, "late", &source().to_string()))
            .unwrap_err()
            .code,
        "profile_editor_read_only"
    );
}
#[test]
fn source_same_millisecond_relation_app_rules_and_lifecycle_changes_invalidate_original_binding() {
    for case in 0..5 {
        let f = fixture(vec![profile(false)], vec![]);
        let before = snapshot(&f.state, Some("manual")).unwrap();
        match case {
            0 => f.state.proxy_configs().lock().unwrap()[0].name = "same-ms-update".into(),
            1 => {
                let p = current(&f);
                app_database::save_domain_data(f.dir.path(), &p, &[]).unwrap();
            }
            2 => {
                let path = f.dir.path().join("app-config.json");
                let a = f.state.app_config().lock().unwrap().clone();
                let mut b = a.clone();
                b.local_proxy.port += 1;
                app_config_store::save(&path, &b).unwrap();
                app_config_store::save(&path, &a).unwrap();
            }
            3 => {
                let rules:crate::models::rule_set::RuleSetProfile=serde_json::from_value(json!({"id":"r","name":"r","enabled":false,"semanticIr":{"version":1,"rules":[]},"updatedAtUnixMs":1})).unwrap();
                domain_store::save_rule_sets_to_dir(f.dir.path(), &[rules]).unwrap();
                domain_store::save_rule_sets_to_dir(f.dir.path(), &[]).unwrap();
            }
            _ => f.state.client_core_instance_started(),
        }
        assert!(save(&f.state, input(&before, "late", &source().to_string())).is_err());
        assert_eq!(current(&f)[0].name, "original");
        assert!(f.state.configuration().transaction().is_none());
    }
}
#[test]
fn native_schema_failure_and_unauthorized_dependencies_preserve_original_name_body_and_no_runtime_receipt(
) {
    let f = fixture(vec![profile(false)], vec![]);
    let before = snapshot(&f.state, Some("manual")).unwrap();
    for text in [
        "{}",
        r#"{"route":{"final":{"type":"direct"},"geoip_database":"/secret-canary.mmdb"}}"#,
        r#"{"route":{"final":{"type":"route","outbound":"secret-canary-missing"}}}"#,
    ] {
        let error = save(&f.state, input(&before, "must-not-save", text)).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("secret-canary"));
        assert_eq!(current(&f)[0].name, "original");
        assert_eq!(current(&f)[0].content, Some(source()));
    }
}
#[test]
fn actual_sqlite_failure_keeps_original_disk_memory_and_old_binding_without_compensation() {
    let f = fixture(vec![profile(true)], vec![]);
    let before = snapshot(&f.state, Some("manual")).unwrap();
    let c = rusqlite::Connection::open(f.dir.path().join("znet-sink.db")).unwrap();
    c.execute_batch("CREATE TRIGGER reject_manual BEFORE INSERT ON proxy_configs BEGIN SELECT RAISE(FAIL,'fixture'); END;").unwrap();
    assert!(save(&f.state, input(&before, "not-saved", &source().to_string())).is_err());
    assert_eq!(current(&f)[0].name, "original");
    assert_eq!(f.state.proxy_configs().lock().unwrap()[0].name, "original");
    assert_eq!(
        snapshot(&f.state, Some("manual")).unwrap().binding,
        before.binding
    );
    assert!(f.state.configuration().transaction().is_none());
}
#[test]
fn concurrent_saves_using_one_original_binding_publish_at_most_one_candidate() {
    use std::sync::{Arc, Barrier};
    let f = fixture(vec![profile(false)], vec![]);
    let before = snapshot(&f.state, Some("manual")).unwrap();
    let state = Arc::new(f.state);
    let start = Arc::new(Barrier::new(3));
    let handles: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|name| {
            let state = state.clone();
            let start = start.clone();
            let input = input(&before, name, &source().to_string());
            std::thread::spawn(move || {
                start.wait();
                save(&state, input)
            })
        })
        .collect();
    start.wait();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let persisted = app_database::load_domain_data(f.dir.path()).unwrap();
    assert!(matches!(
        persisted.proxy_configs[0].name.as_str(),
        "first" | "second"
    ));
}
#[test]
fn desktop_file_read_returns_exact_text_without_parsing_or_dependency_access_and_native_rejects_paths(
) {
    let manager = znet_client_core::capability::Manager::default();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selected.json");
    let raw = "not-even-json secret-canary 18446744073709551615";
    std::fs::write(&path, raw).unwrap();
    assert_eq!(
        read_file(&manager, path.to_string_lossy().into(), false).unwrap(),
        raw
    );
    assert!(manager
        .operations()
        .iter()
        .all(|o| o.capability == "file.read"));
    let count = manager.operations().len();
    assert!(read_file(&manager, "/secret-canary-unselected".into(), true).is_err());
    assert_eq!(manager.operations().len(), count);
    std::fs::write(&path, [0xff]).unwrap();
    let error = read_file(&manager, path.to_string_lossy().into(), false).unwrap_err();
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("selected.json"));
}

#[test]
fn lost_create_delivery_can_only_read_the_original_fixed_target_and_never_repeat_creation() {
    let f = fixture(vec![], vec![]);
    let original = snapshot(&f.state, None).unwrap();
    let target = original.create_target_id.clone().unwrap();
    assert_eq!(
        target,
        format!("manual-{}", original.binding.as_ref().unwrap())
    );
    // Simulate delivery loss after the actual local save: only retain the
    // pre-submit snapshot and its fixed target, not the returned receipt.
    save(
        &f.state,
        input(&original, "original create", &source().to_string()),
    )
    .unwrap();
    let observed = snapshot(&f.state, Some(&target)).unwrap();
    assert_eq!(observed.target_id.as_deref(), Some(target.as_str()));
    assert_eq!(observed.name, "original create");
    assert_eq!(
        observed.source_json.as_deref(),
        Some(workspace::pretty(&source()).unwrap().as_str())
    );
    assert!(save(
        &f.state,
        input(&original, "duplicate", &source().to_string())
    )
    .is_err());
    assert_eq!(current(&f).len(), 1);
    assert_eq!(current(&f)[0].name, "original create");
    // A fresh create preview has a different relation epoch/target. A new
    // host session likewise cannot accidentally recreate the original target.
    assert_ne!(
        snapshot(&f.state, None).unwrap().create_target_id,
        Some(target.clone())
    );
    let loaded = app_database::load_domain_data(f.dir.path()).unwrap();
    let app = app_config_store::load_or_default(&f.dir.path().join("app-config.json")).unwrap();
    let next = AppState::with_domain_data(
        app,
        loaded.proxy_configs,
        loaded.subscriptions,
        vec![],
        vec![],
    );
    workspace::recovery::set_test_directory(&next, f.dir.path().to_path_buf());
    assert_ne!(
        snapshot(&next, None).unwrap().create_target_id,
        Some(target)
    );
    assert!(f.state.configuration().transaction().is_none());
}
