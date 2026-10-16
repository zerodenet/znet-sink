use super::*;
use crate::models::proxy_config::ProxyConfigCapabilities;
use crate::services::app_database;
use serde_json::{json, Value};

struct Fixture {
    state: AppState,
    dir: tempfile::TempDir,
}
fn resource(id: &str) -> RuleSetProfile {
    serde_json::from_value(json!({"id":id,"name":id,"enabled":true,"semanticIr":{"rules":[]},"updatedAtUnixMs":1,
        "commonBinding":{"enabled":true,"action":"direct","order":0},
        "artifact":{"path":"unread-resource-canary.zrs","majorVersion":1,"minorVersion":0,"checksum":0,"fileSize":1,"entryCount":1,"builtAtUnixMs":1}})).unwrap()
}
fn publish_resource(dir: &std::path::Path, resource: &mut RuleSetProfile) {
    let ir=zero_rule::protocol::decode_json(json!({"version":1,"name":resource.id,"rules":[{"type":"domain_exact","value":"example.test"}]}).to_string().as_bytes()).unwrap();
    let (compiled, _) = zero_rule::RuleSetCompiler.compile(ir).unwrap();
    let bytes = zero_rule::zrs::encode(&compiled).unwrap();
    let info = zero_rule::zrs::verify(&bytes, zero_rule::zrs::VerifyMode::FullChecksum).unwrap();
    let directory = dir.join("rule-artifacts").join(&resource.id);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("1-{:08x}.zrs", info.body_checksum));
    std::fs::write(&path, &bytes).unwrap();
    resource.artifact = Some(crate::models::rule_set::ZrsArtifact {
        path: path.to_string_lossy().into_owned(),
        major_version: info.major_version,
        minor_version: info.minor_version,
        checksum: info.body_checksum,
        file_size: info.file_size,
        entry_count: info.entry_count(),
        built_at_unix_ms: 1,
    });
}
fn source(references: bool) -> Value {
    let tag = native_rules::common_tag("old");
    let mut s = json!({"mode":{"type":"rule"},"route":{"final":{"type":"direct"},"rules":[]},"runtime":{"udp_upstream_idle_timeout_seconds":u64::MAX}});
    if references {
        s["route"]["rules"] = json!([{"condition":{"type":"or","items":[{"type":"rule_set","tag":tag},{"type":"domain","values":["keep.test"]}]},"action":{"type":"reject"},"mode":"flow"}]);
        s["route"]["bypass"] = json!([{"type":"rule_set","tag":tag}]);
    }
    s
}
fn fixture(references: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let p = ProxyConfigProfile {
        id: "profile".into(),
        name: "Ordinary".into(),
        kernel: "zero".into(),
        format: "zero-json".into(),
        path: None,
        content: Some(source(references)),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: ProxyConfigCapabilities::default(),
    };
    let mut app = AppConfig::default();
    crate::services::bypass::normalize(&mut app).unwrap();
    app.local_proxy.source_proxy_config_id = Some("runtime-owned".into());
    app.client_edits = Some(BTreeMap::from([(
        "routing.injectCommonRules".into(),
        json!(true),
    )]));
    app.core.socket = Some(
        dir.path()
            .join("no-runtime.sock")
            .to_string_lossy()
            .into_owned(),
    );
    let mut rules = vec![resource("old"), resource("target")];
    for resource in &mut rules {
        publish_resource(dir.path(), resource);
    }
    app_database::save_domain_data(dir.path(), std::slice::from_ref(&p), &[]).unwrap();
    app_config_store::save(&dir.path().join("app-config.json"), &app).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &rules).unwrap();
    let state = AppState::with_domain_data(app, vec![p], vec![], rules, vec![]);
    workspace::recovery::set_test_directory(&state, dir.path().to_path_buf());
    Fixture { state, dir }
}
fn input(f: &Fixture, target: Option<&str>) -> PreviewInput {
    let s = snapshot(&f.state, "profile", "old").unwrap();
    PreviewInput {
        profile_id: s.profile_id,
        rule_set_id: s.rule_set_id,
        binding: s.binding.unwrap(),
        target_id: target.map(str::to_owned),
    }
}
fn plan(f: &Fixture) -> Plan {
    preview(&f.state, input(f, Some("target"))).unwrap()
}
fn rules_on_disk(f: &Fixture) -> Vec<RuleSetProfile> {
    domain_store::load_all_from_dir(f.dir.path())
        .unwrap()
        .rule_sets
}
fn app_on_disk(f: &Fixture) -> AppConfig {
    app_config_store::load_or_default(&f.dir.path().join("app-config.json")).unwrap()
}

#[test]
fn real_files_reopen_source_sqlite_unchanged_and_reference_projection_precedes_retirement() {
    let f = fixture(true);
    let before = app_database::load_domain_data(f.dir.path()).unwrap();
    #[cfg(unix)]
    let listener =
        std::os::unix::net::UnixListener::bind(f.dir.path().join("no-runtime.sock")).unwrap();
    #[cfg(unix)]
    listener.set_nonblocking(true).unwrap();
    let p = plan(&f);
    assert_eq!(p.references.len(), 2);
    let saved = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(saved.state, "saved_pending_apply");
    assert!(saved.app_config_published && saved.rule_set_removed);
    let disk = app_on_disk(&f);
    assert_eq!(
        disk.local_proxy.source_proxy_config_id.as_deref(),
        Some("runtime-owned")
    );
    assert_eq!(
        rules_on_disk(&f)
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["target"]
    );
    let (_, projected) =
        super::super::local_edits::resolve(&disk, Some("profile"), &source(true)).unwrap();
    assert_eq!(
        projected
            .pointer("/route/rules/0/condition/items/0/tag")
            .unwrap(),
        &json!(native_rules::common_tag("target"))
    );
    assert_eq!(
        projected["runtime"]["udp_upstream_idle_timeout_seconds"].as_u64(),
        Some(u64::MAX)
    );
    let after = app_database::load_domain_data(f.dir.path()).unwrap();
    assert_eq!(
        workspace::digest(&before.proxy_configs).unwrap(),
        workspace::digest(&after.proxy_configs).unwrap()
    );
    let absent = snapshot(&f.state, "profile", "old").unwrap();
    assert!(!absent.resource_exists);
    assert_eq!(absent.binding, saved.after_binding);
    #[cfg(unix)]
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn unused_resource_removal_does_not_rewrite_app_config() {
    let f = fixture(false);
    let before = std::fs::read(f.dir.path().join("app-config.json")).unwrap();
    let s = snapshot(&f.state, "profile", "old").unwrap();
    assert!(s.references.is_empty());
    let p = preview(&f.state, input(&f, None)).unwrap();
    let saved = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(saved.state, "saved_pending_apply");
    assert!(!saved.app_config_published);
    assert!(saved.rule_set_removed);
    assert_eq!(
        std::fs::read(f.dir.path().join("app-config.json")).unwrap(),
        before
    );
}
#[test]
fn references_require_explicit_target_and_fixed_plan_never_adopts_later_selection() {
    let f = fixture(true);
    assert_eq!(
        preview(&f.state, input(&f, None)).unwrap_err().code,
        "rule_migration_target_required"
    );
    let p = plan(&f);
    assert_eq!(p.target_id.as_deref(), Some("target"));
    let result = save(&f.state, &p.plan_id).unwrap();
    let bytes = std::fs::read(f.dir.path().join("rule-sets.json")).unwrap();
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().after_binding,
        result.after_binding
    );
    assert_eq!(
        std::fs::read(f.dir.path().join("rule-sets.json")).unwrap(),
        bytes
    );
}
#[test]
fn app_history_aba_rejects_old_plan_even_when_value_returns() {
    let f = fixture(true);
    let p = plan(&f);
    let old = app_on_disk(&f);
    let mut next = old.clone();
    next.ui.theme = "dark".into();
    app_config_store::save(&f.dir.path().join("app-config.json"), &next).unwrap();
    app_config_store::save(&f.dir.path().join("app-config.json"), &old).unwrap();
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    assert_eq!(rules_on_disk(&f).len(), 2);
}
#[test]
fn rule_file_history_aba_rejects_old_plan() {
    let f = fixture(true);
    let p = plan(&f);
    let old = rules_on_disk(&f);
    let mut next = old.clone();
    next[1].name = "changed".into();
    domain_store::save_rule_sets_to_dir(f.dir.path(), &next).unwrap();
    domain_store::save_rule_sets_to_dir(f.dir.path(), &old).unwrap();
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
}
#[test]
fn relational_aba_rejects_same_material_with_new_epoch() {
    let f = fixture(true);
    let p = plan(&f);
    let graph = app_database::load_domain_data(f.dir.path()).unwrap();
    app_database::save_domain_data(f.dir.path(), &graph.proxy_configs, &graph.subscriptions)
        .unwrap();
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
}
#[test]
fn target_disable_after_preview_does_not_delete_old_resource() {
    let f = fixture(true);
    let p = plan(&f);
    let mut next = rules_on_disk(&f);
    next[1].enabled = false;
    domain_store::save_rule_sets_to_dir(f.dir.path(), &next).unwrap();
    *lock(f.state.rule_sets(), "rule_set").unwrap() = next;
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    assert_eq!(rules_on_disk(&f).len(), 2);
}
#[test]
fn lifecycle_generation_change_rejects_old_plan() {
    let f = fixture(true);
    let p = plan(&f);
    f.state.client_core_instance_started();
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
}
#[test]
fn phase_one_failure_keeps_both_assets_and_original_unknown_operation_without_replay() {
    let f = fixture(true);
    let p = plan(&f);
    f.state
        .rule_migration()
        .fail_after_app
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let saved = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(saved.state, "submitted_unknown");
    assert!(saved.app_config_published);
    assert!(!saved.rule_set_removed);
    assert!(saved.after_binding.is_none());
    assert_eq!(rules_on_disk(&f).len(), 2);
    assert!(app_on_disk(&f).profile_edits["profile"].contains_key("ruleSetReferences"));
    let app = std::fs::read(f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert_eq!(
        std::fs::read(f.dir.path().join("app-config.json")).unwrap(),
        app
    );
    assert!(discard(&f.state, &p.plan_id).is_err());
    assert_eq!(
        preview(&f.state, input(&f, Some("target")))
            .unwrap_err()
            .code,
        "rule_migration_unknown"
    );
}
#[test]
fn phase_two_missing_delivery_remains_unknown_despite_absent_asset() {
    let f = fixture(true);
    let p = plan(&f);
    f.state
        .rule_migration()
        .fail_after_rules
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let s = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(s.state, "submitted_unknown");
    assert!(s.app_config_published && s.rule_set_removed);
    assert!(s.after_binding.is_none());
    assert_eq!(rules_on_disk(&f).len(), 1);
    assert_eq!(
        status(&f.state, &p.plan_id).unwrap().state,
        "submitted_unknown"
    );
}
#[test]
fn later_writer_is_not_the_original_operation_receipt() {
    fn later(state: &AppState) {
        let dir = workspace::recovery::directory(state).unwrap();
        let mut app = lock(state.app_config(), "app_config").unwrap();
        app.ui.theme = "dark".into();
        app_config_store::save(&dir.join("app-config.json"), &app).unwrap();
    }
    let f = fixture(true);
    let p = plan(&f);
    *lock(&f.state.rule_migration().after_publish, "test").unwrap() = Some(later);
    let s = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(s.state, "submitted_unknown");
    assert!(s.after_binding.is_none());
}
#[test]
fn builtin_and_managed_sources_are_never_retired() {
    for managed in [false, true] {
        let f = fixture(false);
        {
            let mut rules = lock(f.state.rule_sets(), "rules").unwrap();
            if managed {
                rules[0].managed_by_subscription_id = Some("sub".into());
            } else {
                rules[0].built_in = true;
            }
            domain_store::save_rule_sets_to_dir(f.dir.path(), &rules).unwrap();
        }
        let s = snapshot(&f.state, "profile", "old").unwrap();
        assert!(!s.blockers.is_empty());
        assert!(preview(&f.state, input(&f, None)).is_err());
        assert_eq!(rules_on_disk(&f).len(), 2);
    }
}
#[test]
fn expired_preview_cannot_publish_and_discarded_plan_cannot_reconstruct() {
    let f = fixture(false);
    let p = preview(&f.state, input(&f, None)).unwrap();
    lock(&f.state.rule_migration().plans, "plans")
        .unwrap()
        .get_mut(&p.plan_id)
        .unwrap()
        .deadline = Instant::now() - Duration::from_secs(1);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap_err().code,
        "rule_migration_expired"
    );
    discard(&f.state, &p.plan_id).unwrap();
    assert!(status(&f.state, &p.plan_id).is_err());
    assert_eq!(rules_on_disk(&f).len(), 2);
}

#[test]
fn replacement_artifact_change_after_preview_rejects_without_publication() {
    let f = fixture(true);
    let p = plan(&f);
    let target = rules_on_disk(&f).pop().unwrap();
    std::fs::write(&target.artifact.unwrap().path, b"changed bytes").unwrap();
    let result = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(result.state, "rejected");
    assert_eq!(rules_on_disk(&f).len(), 2);
    assert!(!app_on_disk(&f)
        .profile_edits
        .get("profile")
        .is_some_and(|e| e.contains_key("ruleSetReferences")));
}
#[test]
fn migrated_unchanged_source_reaches_real_workspace_native_compilation_of_verified_target() {
    let f = fixture(true);
    let p = plan(&f);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    // Raw source cannot validate before its client definitions are composed.
    assert!(workspace::parse(&source(true).to_string()).is_err());
    let snapshot = workspace::local_snapshot(&f.state).unwrap();
    let input = crate::models::config_workspace::ConfigWorkspaceDraftInput {
        binding: snapshot.binding.unwrap(),
        source_json: source(true).to_string(),
    };
    let (_, original, candidate) = workspace::from_input(&f.state, &input).unwrap();
    assert_eq!(original, source(true));
    assert_eq!(
        candidate["route"]["rules"][0]["condition"]["items"][0]["tag"],
        native_rules::common_tag("target")
    );
    workspace::validate(&candidate).unwrap();
    let duplicate = crate::models::config_workspace::ConfigWorkspaceDraftInput {
        source_json: format!(
            "{{\"route\":{},{}",
            source(true)["route"],
            &input.source_json[1..]
        ),
        binding: input.binding.clone(),
    };
    assert!(workspace::from_input(&f.state, &duplicate).is_err());
    let mut changed = source(true);
    changed["route"]["rule_sets"] = json!([{"tag":"not-authorized","type":"file","path":"never-read-new-dependency","format":"zrs"}]);
    let changed = crate::models::config_workspace::ConfigWorkspaceDraftInput {
        source_json: changed.to_string(),
        ..input
    };
    assert!(workspace::from_input(&f.state, &changed).is_err());
    assert_eq!(
        app_database::load_domain_data(f.dir.path())
            .unwrap()
            .proxy_configs[0]
            .content
            .as_ref(),
        Some(&source(true))
    );
}
