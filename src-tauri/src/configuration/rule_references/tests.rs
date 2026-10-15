use super::*;
use crate::models::{
    app_config::AppConfig,
    proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile},
    rule_set::{CommonRuleAction, CommonRuleBinding, RuleSetProfile, ZrsArtifact},
};

const SECRET: &str = "secret-canary-never-echo";
fn source() -> Value {
    json!({"mode":{"type":"rule"},"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],
        "route":{"rules":[],"final":{"type":"direct"}},
        "runtime":{"log":{"level":"info","files":[{"path":SECRET,"max_bytes":u64::MAX}]}}})
}
fn condition(tag: &str) -> Value {
    json!({"type":"rule_set","tag":tag})
}
fn rule(tag: &str) -> Value {
    json!({"condition":condition(tag),"action":{"type":"direct"},"mode":"flow"})
}
fn profile(id: &str, value: Value) -> ProxyConfigProfile {
    ProxyConfigProfile {
        id: id.into(),
        name: id.into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(value),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: ProxyConfigCapabilities::default(),
    }
}
fn resource(id: &str) -> RuleSetProfile {
    RuleSetProfile {
        id: id.into(),
        name: id.into(),
        enabled: true,
        built_in: false,
        provenance: None,
        managed_by_subscription_id: None,
        common_binding: Some(CommonRuleBinding {
            enabled: true,
            action: CommonRuleAction::Reject,
            order: 7,
        }),
        semantic_ir: json!({"version":1,"rules":[]}),
        source: None,
        source_state: Default::default(),
        artifact: Some(ZrsArtifact {
            path: format!("/{id}.zrs"),
            major_version: 1,
            minor_version: 0,
            checksum: 1,
            file_size: 1,
            entry_count: 1,
            built_at_unix_ms: 1,
        }),
        updated_at_unix_ms: 1,
        last_sync_at_unix_ms: None,
        last_error: None,
    }
}
fn replacement() -> Vec<Replacement> {
    vec![Replacement {
        from_tag: "old".into(),
        to_tag: "new".into(),
    }]
}

#[test]
fn scans_all_native_locations_and_rewrites_only_known_conditions_losslessly() {
    let mut value = source();
    value["route"]["rules"] = json!([{"condition":{"type":"and","items":[condition("old"),{"type":"or","items":[condition("other"),condition("old")]}]},"action":{"type":"direct"},"mode":"translate"}]);
    value["route"]["bypass"] = json!([condition("old")]);
    value["runtime"]["dns"] = json!({"servers":{"old":{"type":"system"}},"default_server":"old","dispatch":[{"condition":condition("old"),"server":"old"}]});
    let refs = scan(&value).unwrap();
    assert_eq!(refs.len(), 5);
    assert!(refs.iter().any(|r| r.kind == "bypass"));
    assert!(refs
        .iter()
        .any(|r| r.field_path == "runtime.dns.dispatch[0].condition.tag"));
    let next = rewrite(&value, &replacement()).unwrap();
    assert_eq!(next["runtime"]["dns"]["default_server"], "old");
    assert_eq!(next["runtime"]["dns"]["dispatch"][0]["server"], "old");
    assert_eq!(
        next["runtime"]["log"]["files"][0]["max_bytes"].as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(next["route"]["rules"][0]["mode"], "translate");
    assert_eq!(
        scan(&next)
            .unwrap()
            .iter()
            .filter(|r| r.tag == "new")
            .count(),
        4
    );
    assert_eq!(value["route"]["bypass"][0]["tag"], "old");
}
#[test]
fn native_dependency_declarations_are_deserialized_without_reading_files() {
    let mut value = source();
    value["route"]["rule_sets"] =
        json!([{"tag":"old","type":"file","path":"/does/not/exist/private-fifo","format":"zrs"}]);
    value["route"]["rules"] = json!([rule("old")]);
    assert_eq!(scan(&value).unwrap().len(), 1);
    assert_eq!(
        rewrite(&value, &replacement()).unwrap()["route"]["rule_sets"],
        value["route"]["rule_sets"]
    );
}
#[test]
fn disabled_overlay_rows_are_scanned_and_preserved() {
    let value = source();
    let metadata = json!({"version":1,"sourceDigest":super::super::route_rules::source_digest(&value).unwrap(),"rows":[{"id":"disabled","enabled":false,"rule":rule("old")}]});
    let refs = scan_route_rows(&metadata).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].kind, "route_overlay");
    let next = rewrite_route_rows(&metadata, &replacement()).unwrap();
    assert_eq!(next["rows"][0]["enabled"], false);
    assert_eq!(next["rows"][0]["rule"]["condition"]["tag"], "new");
    assert_eq!(next["sourceDigest"], metadata["sourceDigest"]);
}
#[test]
fn source_digest_is_not_rebound_by_apply_or_metadata_validation() {
    let from = super::super::rules::common_tag("old");
    let to = super::super::rules::common_tag("new");
    let mut value = source();
    value["route"]["rules"] = json!([rule(&from)]);
    let metadata = metadata(
        &value,
        &[Replacement {
            from_tag: from,
            to_tag: to.clone(),
        }],
    )
    .unwrap();
    validate_metadata(&metadata).unwrap();
    assert_eq!(
        apply(&value, &value, &metadata).unwrap()["route"]["rules"][0]["condition"]["tag"],
        to
    );
    let mut changed = value.clone();
    changed["route"]["final"] = json!({"type":"reject"});
    assert!(apply(&changed, &changed, &metadata).is_err());
}
#[test]
fn unknown_and_legacy_opaque_conditions_fail_closed_without_canary_errors() {
    for condition in [
        json!({"type":SECRET,"tag":SECRET}),
        json!({"type":"not","condition":condition("old")}),
        json!({"rule_set":"old"}),
        json!({"type":"rule_set","tag":"old","unknown":SECRET}),
    ] {
        let mut value = source();
        value["route"]["rules"] = json!([{"condition":condition,"action":{"type":"direct"}}]);
        let error = scan(&value).unwrap_err();
        assert!(!serde_json::to_string(&error).unwrap().contains(SECRET));
    }
}
#[test]
fn replacement_duplicate_identity_depth_and_reference_budgets_are_enforced() {
    let value = source();
    assert!(rewrite(
        &value,
        &[replacement()[0].clone(), replacement()[0].clone()]
    )
    .is_err());
    assert!(metadata(
        &value,
        &[Replacement {
            from_tag: "x".into(),
            to_tag: "x".into()
        }]
    )
    .is_err());
    let mut nested = condition("old");
    for _ in 0..66 {
        nested = json!({"type":"and","items":[nested]});
    }
    let mut deep = source();
    deep["route"]["bypass"] = json!([nested]);
    assert!(scan(&deep).is_err());
    let mut huge = source();
    huge["route"]["bypass"] = json!(vec![condition("old"); 4097]);
    assert!(scan(&huge).is_err());
    assert!(scan_dns(&json!({"dispatch":[],"padding":"x".repeat(MAX_BYTES)})).is_err());
}
#[test]
fn prepare_migrates_current_source_and_dns_in_app_only_without_moving_common_binding() {
    let rules = vec![resource("old"), resource("new")];
    let old = super::super::rules::common_tag("old");
    let new = super::super::rules::common_tag("new");
    let mut source = source();
    source["route"]["rules"] = json!([rule(&old)]);
    let profiles = vec![profile("p", source.clone())];
    let mut app = AppConfig::default();
    app.client_edits.as_mut().unwrap().insert("dns".into(),json!({"enabled":true,"config":{"dispatch":[{"condition":condition(&old),"server":"system"}],"unknown":u64::MAX}}));
    let result = prepare(&app, &profiles, &[], &rules, Some("p"), "old", Some("new")).unwrap();
    assert!(result.blockers.is_empty(), "{:?}", result.blockers);
    assert_eq!(result.references.len(), 2);
    assert!(result.common_binding_removed);
    assert_eq!(
        result.app.client_edits.as_ref().unwrap()["dns"]["config"]["dispatch"][0]["condition"]
            ["tag"],
        new
    );
    assert_eq!(
        result.app.client_edits.as_ref().unwrap()["dns"]["config"]["unknown"].as_u64(),
        Some(u64::MAX)
    );
    let metadata = &result.app.profile_edits["p"]["ruleSetReferences"];
    assert_eq!(
        apply(&source, &source, metadata).unwrap()["route"]["rules"][0]["condition"]["tag"],
        new
    );
    assert_eq!(profiles[0].content.as_ref(), Some(&source));
    assert_eq!(rules[1].common_binding.as_ref().unwrap().order, 7);
}
#[test]
fn prepare_inventory_without_target_is_not_blocked_just_for_missing_selection() {
    let mut source = source();
    source["route"]["rules"] = json!([rule(&super::super::rules::common_tag("old"))]);
    let app = AppConfig::default();
    let result = prepare(
        &app,
        &[profile("p", source)],
        &[],
        &[resource("old")],
        Some("p"),
        "old",
        None,
    )
    .unwrap();
    assert_eq!(result.references.len(), 1);
    assert!(result.blockers.is_empty());
    assert_eq!(result.app, app);
}
#[test]
fn prepare_blocks_cross_profile_unknown_ownership_and_native_tag_collisions() {
    let old = super::super::rules::common_tag("old");
    let mut value = source();
    value["route"]["bypass"] = json!([condition(&old)]);
    let app = AppConfig::default();
    let rules = vec![resource("old"), resource("new")];
    let profiles = vec![profile("a", source()), profile("b", value.clone())];
    let result = prepare(&app, &profiles, &[], &rules, Some("a"), "old", Some("new")).unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "cross_profile_reference"));
    assert_eq!(result.app, app);
    let mut protected = profile("protected", Value::String(SECRET.into()));
    protected.content = None;
    let result = prepare(&app, &[protected], &[], &rules, None, "old", Some("new")).unwrap();
    assert_eq!(result.blockers[0].reason_code, "source_uninspectable");
    value["route"]["rule_sets"] = json!([{"tag":old,"type":"file","path":SECRET,"format":"zrs"}]);
    let result = prepare(
        &app,
        &[profile("a", value)],
        &[],
        &rules,
        Some("a"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "generated_tag_collision"));
    assert!(!serde_json::to_string(&result.blockers)
        .unwrap()
        .contains(SECRET));
}
#[test]
fn prepare_keeps_existing_mapping_history_and_refuses_source_drift() {
    let rules = vec![resource("a"), resource("b"), resource("c")];
    let a = super::super::rules::common_tag("a");
    let b = super::super::rules::common_tag("b");
    let c = super::super::rules::common_tag("c");
    let mut value = source();
    value["route"]["rules"] = json!([rule(&a)]);
    let mut app = AppConfig::default();
    app.profile_edits.entry("p".into()).or_default().insert(
        "ruleSetReferences".into(),
        metadata(
            &value,
            &[Replacement {
                from_tag: a.clone(),
                to_tag: b.clone(),
            }],
        )
        .unwrap(),
    );
    let result = prepare(
        &app,
        &[profile("p", value.clone())],
        &[],
        &rules,
        Some("p"),
        "b",
        Some("c"),
    )
    .unwrap();
    assert!(result.blockers.is_empty());
    assert_eq!(
        apply(
            &value,
            &value,
            &result.app.profile_edits["p"]["ruleSetReferences"]
        )
        .unwrap()["route"]["rules"][0]["condition"]["tag"],
        c
    );
    value["runtime"]["log"]["level"] = json!("debug");
    let result = prepare(
        &app,
        &[profile("p", value)],
        &[],
        &rules,
        Some("p"),
        "b",
        Some("c"),
    )
    .unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "metadata_source_changed"));
    assert_eq!(result.app, app);
}
#[test]
fn managed_admission_precedes_body_and_subscription_owner_cannot_be_bypassed() {
    let rules = vec![resource("old"), resource("new")];
    let app = AppConfig::default();
    let mut p = profile("p", Value::String(SECRET.into()));
    p.managed_source = Some(
        serde_json::from_value(json!({"pluginId":"p","providerId":"c","remoteSubscriptionId":"s"}))
            .unwrap(),
    );
    let result = prepare(
        &app,
        std::slice::from_ref(&p),
        &[],
        &rules,
        Some("p"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert_eq!(result.blockers[0].reason_code, "managed_source");
    assert!(!serde_json::to_string(&result.blockers)
        .unwrap()
        .contains(SECRET));
    p.managed_source = None;
    let subscription=serde_json::from_value(json!({"id":"s","name":"s","url":"https://invalid.example/private","enabled":false,"kernel":"zero","format":"json","targetProxyConfigId":"p","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null,"managedSource":{"pluginId":"p","providerId":"c","remoteSubscriptionId":"s"}})).unwrap();
    let result = prepare(
        &app,
        &[p],
        &[subscription],
        &rules,
        Some("p"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert_eq!(result.blockers[0].reason_code, "managed_source");
}
#[test]
fn global_and_dormant_dns_scope_and_actual_common_availability_are_explicit() {
    let old = super::super::rules::common_tag("old");
    let rules = vec![resource("old"), resource("new")];
    let dns = json!({"enabled":false,"config":{"dispatch":[{"condition":condition(&old),"server":"system"}]}});
    let mut app = AppConfig::default();
    app.client_edits
        .as_mut()
        .unwrap()
        .insert("dns".into(), dns.clone());
    let profiles = vec![profile("a", source()), profile("b", source())];
    let result = prepare(&app, &profiles, &[], &rules, Some("a"), "old", Some("new")).unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "global_dns_scope"));
    assert_eq!(result.app, app);
    app.client_edits.as_mut().unwrap().remove("dns");
    app.profile_edits
        .entry("b".into())
        .or_default()
        .insert("dns".into(), dns);
    let result = prepare(&app, &profiles, &[], &rules, Some("a"), "old", Some("new")).unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "legacy_dns_scope"));
    let mut app = AppConfig::default();
    app.client_edits
        .as_mut()
        .unwrap()
        .insert("routing.injectCommonRules".into(), json!(false));
    let mut value = source();
    value["route"]["rules"] = json!([rule(&old)]);
    let result = prepare(
        &app,
        &[profile("a", value)],
        &[],
        &rules,
        Some("a"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "replacement_unavailable"));
}
#[test]
fn resource_identity_never_uses_same_native_name_or_artifact_path_as_authority() {
    let mut value = source();
    value["route"]["rule_sets"] =
        json!([{"tag":"old","type":"file","path":"/old.zrs","format":"zrs"}]);
    value["route"]["rules"] = json!([rule("old")]);
    let result = prepare(
        &AppConfig::default(),
        &[profile("p", value)],
        &[],
        &[resource("old"), resource("new")],
        Some("p"),
        "old",
        None,
    )
    .unwrap();
    assert!(result.references.is_empty());
    assert!(result.blockers.is_empty());
    let error = prepare(&AppConfig::default(), &[], &[], &[], None, "missing", None)
        .err()
        .unwrap();
    assert_eq!(error.code, "not_found");
}
#[test]
fn stale_overlay_unknown_metadata_and_non_gui_reference_metadata_cannot_gain_authority() {
    let mut value = source();
    let old = super::super::rules::common_tag("old");
    value["route"]["rules"] = json!([rule(&old)]);
    let mut app = AppConfig::default();
    app.profile_edits.entry("p".into()).or_default().insert(
        "routeRules".into(),
        json!({"version":1,"sourceDigest":"a".repeat(64),"rows":[]}),
    );
    let result = prepare(
        &app,
        &[profile("p", value.clone())],
        &[],
        &[resource("old"), resource("new")],
        Some("p"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "metadata_source_changed"));
    assert_eq!(result.app, app);
    assert!(metadata(&value, &replacement()).is_err());
    let mut meta = metadata(
        &value,
        &[Replacement {
            from_tag: old,
            to_tag: super::super::rules::common_tag("new"),
        }],
    )
    .unwrap();
    meta["unknown"] = json!(SECRET);
    let error = validate_metadata(&meta).unwrap_err();
    assert!(!serde_json::to_string(&error).unwrap().contains(SECRET));
}

#[test]
fn opaque_potential_dns_references_block_without_rewriting_additive_plain_values() {
    let normal = json!({"dispatch":[{"condition":condition("old"),"server":"old"}],"futureBudget":u64::MAX,"plain":"old"});
    let result = rewrite_dns(&normal, &replacement()).unwrap();
    assert_eq!(result["futureBudget"].as_u64(), Some(u64::MAX));
    assert_eq!(result["plain"], "old");
    for opaque in [
        json!({"type":"rule_set","tag":SECRET}),
        json!({"rule_set":SECRET}),
        json!({"tag":SECRET}),
    ] {
        let mut config = normal.clone();
        config["unknown"] = opaque.clone();
        let error = scan_dns(&config).unwrap_err();
        assert!(!serde_json::to_string(&error).unwrap().contains(SECRET));
        let mut app = AppConfig::default();
        app.client_edits.as_mut().unwrap().insert(
            "dns".into(),
            json!({"enabled":false,"config":{"dispatch":[]},"unknown":opaque}),
        );
        let result = prepare(
            &app,
            &[profile("p", source())],
            &[],
            &[resource("old"), resource("new")],
            Some("p"),
            "old",
            None,
        )
        .unwrap();
        assert!(result
            .blockers
            .iter()
            .any(|b| b.reason_code == "metadata_invalid"));
        assert_eq!(result.app, app);
        assert!(!serde_json::to_string(&result.blockers)
            .unwrap()
            .contains(SECRET));
    }
}

#[test]
fn inventory_overflow_is_explicitly_blocked_and_never_returns_unbounded_rows_or_partial_app() {
    let old = super::super::rules::common_tag("old");
    let mut value = source();
    value["route"]["rules"] = json!(vec![rule(&old); 4096]);
    let mut app = AppConfig::default();
    app.profile_edits.entry("p".into()).or_default().insert("routeRules".into(),json!({"version":1,"sourceDigest":super::super::route_rules::source_digest(&value).unwrap(),"rows":[{"id":"disabled","enabled":false,"rule":rule(&old)}]}));
    let result = prepare(
        &app,
        &[profile("p", value)],
        &[],
        &[resource("old"), resource("new")],
        Some("p"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert_eq!(result.references.len(), 4096);
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "reference_budget_exceeded"));
    assert_eq!(result.app, app);
}

#[cfg(unix)]
#[test]
fn real_fifo_dependency_is_never_opened_by_scan_or_reference_projection() {
    use std::{ffi::CString, io::Write, os::unix::fs::OpenOptionsExt, sync::mpsc, time::Duration};
    let directory = tempfile::tempdir().unwrap();
    let fifo = directory.path().join("must-not-open");
    let name = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: name is a valid NUL-terminated path owned by this test's tmpdir.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut value = source();
    value["route"]["rule_sets"] =
        json!([{"tag":"old","type":"file","path":fifo.to_str().unwrap(),"format":"domain_list"}]);
    value["route"]["rules"] = json!([rule("old")]);
    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(rewrite(&value, &replacement()));
    });
    let result = rx.recv_timeout(Duration::from_secs(2));
    if result.is_err() {
        if let Ok(mut writer) = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&fifo)
        {
            let _ = writer.write_all(b"example.invalid\n");
        }
    }
    assert_eq!(
        result
            .expect("pure reference projection attempted FIFO IO")
            .unwrap()["route"]["rules"][0]["condition"]["tag"],
        "new"
    );
    worker.join().unwrap();
}

#[test]
fn existing_route_overlay_with_unconsumable_disabled_gui_reference_is_blocked_with_inventory() {
    let value = source();
    let old = super::super::rules::common_tag("old");
    let mut app = AppConfig::default();
    app.profile_edits.entry("p".into()).or_default().insert("routeRules".into(),json!({"version":1,"sourceDigest":super::super::route_rules::source_digest(&value).unwrap(),"rows":[{"id":"disabled","enabled":false,"rule":rule(&old)}]}));
    let result = prepare(
        &app,
        &[profile("p", value)],
        &[],
        &[resource("old"), resource("new")],
        Some("p"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "metadata_invalid"));
    assert!(result
        .references
        .iter()
        .any(|r| r.kind == "route_overlay" && r.tag == old));
    assert_eq!(result.app, app);
}

#[test]
fn valid_route_overlay_does_not_prevent_global_dns_only_reference_migration() {
    let value = source();
    let old = super::super::rules::common_tag("old");
    let mut app = AppConfig::default();
    app.profile_edits.entry("p".into()).or_default().insert("routeRules".into(),json!({"version":1,"sourceDigest":super::super::route_rules::source_digest(&value).unwrap(),"rows":[{"id":"ordinary","enabled":true,"rule":{"condition":{"type":"domain","values":["example.test"]},"action":{"type":"direct"}}}]}));
    app.client_edits.as_mut().unwrap().insert("dns".into(),json!({"enabled":true,"config":{"dispatch":[{"condition":condition(&old),"server":"system"}]}}));
    let result = prepare(
        &app,
        &[profile("p", value)],
        &[],
        &[resource("old"), resource("new")],
        Some("p"),
        "old",
        Some("new"),
    )
    .unwrap();
    assert!(result.blockers.is_empty(), "{:?}", result.blockers);
    assert_eq!(result.references.len(), 1);
    assert_eq!(result.references[0].kind, "global_dns");
    assert_eq!(
        result.app.profile_edits["p"]["routeRules"],
        app.profile_edits["p"]["routeRules"]
    );
    assert_eq!(
        result.app.client_edits.as_ref().unwrap()["dns"]["config"]["dispatch"][0]["condition"]
            ["tag"],
        super::super::rules::common_tag("new")
    );
}

#[test]
fn legacy_profile_common_opt_out_is_resolved_without_publishing_unrelated_migration() {
    let mut value = source();
    value["route"]["rules"] = json!([rule(&super::super::rules::common_tag("old"))]);
    let mut app = AppConfig {
        client_edits: None,
        ..Default::default()
    };
    app.profile_edits
        .entry("p".into())
        .or_default()
        .insert("routing.injectCommonRules".into(), json!(false));
    let rules = vec![resource("old"), resource("new")];
    let profiles = vec![profile("p", value)];
    let result = prepare(&app, &profiles, &[], &rules, Some("p"), "old", Some("new")).unwrap();
    assert!(result
        .blockers
        .iter()
        .any(|b| b.reason_code == "replacement_unavailable"));
    assert_eq!(result.app, app);
    app.profile_edits
        .get_mut("p")
        .unwrap()
        .insert("routing.injectCommonRules".into(), json!(true));
    let result = prepare(&app, &profiles, &[], &rules, Some("p"), "old", Some("new")).unwrap();
    assert!(result.blockers.is_empty(), "{:?}", result.blockers);
    assert!(result.app.client_edits.is_none());
    assert_eq!(
        result.app.profile_edits["p"]["routing.injectCommonRules"],
        true
    );
}
