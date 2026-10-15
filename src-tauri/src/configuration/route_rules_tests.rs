use super::*;
use crate::{
    configuration::{composition, local_edits},
    models::app_config::AppConfig,
};
use serde_json::json;
use std::collections::BTreeMap;

const CANARY: &str = "private-canary-token-must-not-escape";
fn base() -> Value {
    json!({"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],
        "route":{"rules":[],"final":{"type":"direct"},
            "url_rewrite":[{"from":"old.example","to":"new.example"}]},
        "runtime":{"log":{"level":"info","files":[{"path":"not-created-by-pure-projection.log","max_bytes":u64::MAX}]}}})
}
fn rule(domain: &str) -> Value {
    json!({"condition":{"type":"domain","values":[domain]},"action":{"type":"direct"},"mode":"flow"})
}
fn row(id: &str, enabled: bool, rule: Value) -> Value {
    json!({"id":id,"enabled":enabled,"rule":rule})
}
fn overlay(base: &Value, rows: Vec<Value>) -> Value {
    json!({"version":1,"sourceDigest":source_digest(base).unwrap(),"rows":rows})
}
fn scoped(app: &AppConfig, id: &str, metadata: Value) -> AppConfig {
    local_edits::candidate(
        app,
        id,
        BTreeMap::from([("routeRules".into(), metadata)]),
        &[],
    )
    .unwrap()
}

#[test]
fn ordered_enabled_projection_preserves_native_values_and_never_exports_ui_metadata() {
    let original = base();
    let advanced = json!({"condition":{"type":"and","items":[
        {"type":"or","items":[{"type":"sni","values":["tls.example"]},{"type":"domain_regex","values":["^api[.]example$"]}]},
        {"type":"ip","values":["192.0.2.0/24"]}]},"action":{"type":"route","outbound":"direct"},"mode":"flow"});
    let metadata = overlay(
        &original,
        vec![
            row("last", true, advanced.clone()),
            row("hidden", false, rule("disabled.example")),
            row("first", true, rule("visible.example")),
        ],
    );
    let result = apply(&original, &metadata).unwrap();
    let mut expected = original.clone();
    expected["route"]["rules"] = json!([advanced, rule("visible.example")]);
    assert_eq!(result, expected);
    assert_eq!(
        result["runtime"]["log"]["files"][0]["max_bytes"].as_u64(),
        Some(u64::MAX)
    );
    assert!(result.get("schema_version").is_none());
    assert_eq!(original, base());
    assert!(result["route"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r.get("id").is_none() && r.get("enabled").is_none()));
    assert_eq!(metadata["rows"].as_array().unwrap().len(), 3);
}

#[test]
fn disabled_rules_still_require_native_schema_and_actual_references() {
    let original = base();
    for invalid_rule in [
        json!({"condition":{"type":"domain","values":[CANARY]},"action":{"type":"route","outbound":CANARY}}),
        json!({"condition":{"type":"domain_regex","values":[format!("{CANARY}[")]},"action":{"type":"direct"}}),
        json!({"condition":{"type":"domain","values":["ok.example"]},"action":{"type":"direct"},(CANARY):true}),
        json!({"condition":{"type":"rule_set","tag":CANARY},"action":{"type":"direct"}}),
    ] {
        let error = apply(
            &original,
            &overlay(&original, vec![row("off", false, invalid_rule)]),
        )
        .unwrap_err();
        assert_eq!(error.code, "native_validation_failed");
        assert!(!serde_json::to_string(&error).unwrap().contains(CANARY));
    }
    assert_eq!(original, base());
}

#[test]
fn full_source_digest_rejects_any_source_edit_without_rebinding_or_partial_mutation() {
    let original = base();
    let metadata = overlay(&original, vec![row("a", true, rule("a.example"))]);
    assert_eq!(
        metadata["sourceDigest"],
        crate::configuration::workspace::digest(&original).unwrap()
    );
    for path in ["route", "runtime", "outbounds"] {
        let mut changed = original.clone();
        changed[path] = json!({"changed":true});
        let before = changed.clone();
        let error = apply(&changed, &metadata).unwrap_err();
        assert_eq!(error.code, "conflict");
        assert_eq!(changed, before);
    }
    assert_eq!(metadata["sourceDigest"], source_digest(&original).unwrap());
}

#[test]
fn envelopes_versions_identity_duplicates_and_budgets_fail_without_echoing_material() {
    let original = base();
    let valid = overlay(&original, vec![row("row:1", true, rule("a.example"))]);
    for malformed in [
        json!(null),
        json!({"version":2,"sourceDigest":source_digest(&original).unwrap(),"rows":[]}),
        json!({"version":1,"sourceDigest":"bad","rows":[]}),
        json!({"version":1,"sourceDigest":source_digest(&original).unwrap(),"rows":[],(CANARY):true}),
        overlay(
            &original,
            vec![
                row("a", true, rule("a.example")),
                row("a", false, rule("b.example")),
            ],
        ),
        overlay(&original, vec![row(" ", true, rule("a.example"))]),
        overlay(
            &original,
            vec![row(&"a".repeat(129), true, rule("a.example"))],
        ),
        overlay(
            &original,
            vec![json!({"id":"a","enabled":"true","rule":rule("a.example")})],
        ),
        overlay(&original, vec![row("a", true, json!(CANARY))]),
        overlay(
            &original,
            (0..=MAX_ROWS)
                .map(|n| row(&format!("r{n}"), true, rule("a.example")))
                .collect(),
        ),
        overlay(
            &original,
            vec![row("large", true, rule(&"x".repeat(MAX_BYTES)))],
        ),
    ] {
        let error = validate(&malformed).unwrap_err();
        assert!(!serde_json::to_string(&error).unwrap().contains(CANARY));
    }
    assert!(validate(&valid).is_ok());
}

#[test]
fn empty_and_all_disabled_overrides_mean_explicit_empty_native_rules() {
    let original = base();
    for rows in [vec![], vec![row("a", false, rule("a.example"))]] {
        let result = apply(&original, &overlay(&original, rows)).unwrap();
        assert_eq!(result["route"]["rules"], json!([]));
        assert_eq!(result["route"]["final"], original["route"]["final"]);
    }
}

#[test]
fn rules_and_geoip_dependencies_are_denied_before_native_file_compilation() {
    for dependency in ["rule_sets", "geoip_database"] {
        let mut source = base();
        source["route"][dependency] = if dependency == "rule_sets" {
            json!([{"tag":"shared","type":"file","path":format!("/unapproved/{CANARY}"),"format":"zrs"}])
        } else {
            json!(format!("/unapproved/{CANARY}"))
        };
        let error = apply(
            &source,
            &overlay(&source, vec![row("a", false, rule("a.example"))]),
        )
        .unwrap_err();
        assert_eq!(
            error.details.as_ref().unwrap()["errors"][0]["reasonCode"],
            "dependency_authorization_required"
        );
        assert!(!serde_json::to_string(&error).unwrap().contains(CANARY));
    }
}

#[test]
fn scoped_storage_migration_reset_and_global_import_never_cross_profile_boundaries() {
    let original = base();
    let a = overlay(&original, vec![row("a", true, rule("a.example"))]);
    let b = overlay(&original, vec![row("b", true, rule("b.example"))]);
    let app = scoped(
        &scoped(&AppConfig::default(), "a", a.clone()),
        "b",
        b.clone(),
    );
    assert!(app
        .client_edits
        .as_ref()
        .unwrap()
        .get("routeRules")
        .is_none());
    assert_eq!(app.profile_edits["a"]["routeRules"], a);
    assert_eq!(
        local_edits::resolve(&app, Some("a"), &original).unwrap().1["route"]["rules"],
        json!([rule("a.example")])
    );
    assert_eq!(
        local_edits::resolve(&app, Some("b"), &original).unwrap().1["route"]["rules"],
        json!([rule("b.example")])
    );
    for id in [None, Some("other")] {
        assert_eq!(
            local_edits::resolve(&app, id, &original).unwrap().1,
            original
        );
    }
    let reset = local_edits::candidate(&app, "a", BTreeMap::new(), &["routeRules".into()]).unwrap();
    assert!(!reset.profile_edits.contains_key("a"));
    assert_eq!(reset.profile_edits["b"], app.profile_edits["b"]);
    assert_eq!(
        local_edits::resolve(&reset, Some("a"), &original)
            .unwrap()
            .1,
        original
    );
    let mut refreshed = original.clone();
    refreshed["route"]["rules"] = json!([rule("latest-source.example")]);
    assert_eq!(
        local_edits::resolve(&reset, Some("a"), &refreshed)
            .unwrap()
            .1,
        refreshed,
        "reset removes only this profile overlay and reads the latest source"
    );
    assert!(local_edits::validate_client_edits(
        &app,
        &BTreeMap::from([("routeRules".into(), a.clone())])
    )
    .is_err());
    let mut global = AppConfig::default();
    global
        .client_edits
        .as_mut()
        .unwrap()
        .insert("routeRules".into(), a.clone());
    assert!(local_edits::resolve(&global, Some("a"), &original).is_err());
    let mut legacy = app.clone();
    legacy.client_edits = None;
    legacy
        .profile_edits
        .get_mut("a")
        .unwrap()
        .insert("urlTest.url".into(), json!("https://latency.example/204"));
    assert!(local_edits::migrate_legacy(&mut legacy, Some("a")));
    assert_eq!(legacy.profile_edits["a"]["routeRules"], a);
    assert!(!legacy
        .client_edits
        .as_ref()
        .unwrap()
        .contains_key("routeRules"));
    assert!(legacy
        .client_edits
        .as_ref()
        .unwrap()
        .contains_key("urlTest.url"));
}

#[test]
fn source_binding_is_checked_before_global_projection_and_endpoint_metadata_is_preserved() {
    let source = base();
    let metadata = overlay(&source, vec![row("a", true, rule("a.example"))]);
    let mut app = scoped(&AppConfig::default(), "a", metadata.clone());
    app.profile_edits
        .get_mut("a")
        .unwrap()
        .insert("endpoints".into(), json!({}));
    app = local_edits::candidate(
        &app,
        "a",
        BTreeMap::from([("urlTest.url".into(), json!("https://latency.example/204"))]),
        &[],
    )
    .unwrap();
    let (_, result) = local_edits::resolve(&app, Some("a"), &source).unwrap();
    assert_eq!(result["route"]["rules"], json!([rule("a.example")]));
    assert_eq!(
        result["runtime"]["latency_test_url"],
        "https://latency.example/204"
    );
    assert_eq!(app.profile_edits["a"]["routeRules"], metadata);
    assert_eq!(app.profile_edits["a"]["endpoints"], json!({}));
}

#[test]
fn absent_metadata_keeps_existing_resolver_behavior_without_schema_or_dependency_io() {
    let source = json!({"future-preserved":u64::MAX,"route":{"geoip_database":"not-read-by-existing-empty-edits"}});
    assert_eq!(
        local_edits::resolve(&AppConfig::default(), Some("a"), &source)
            .unwrap()
            .1,
        source
    );
}

#[test]
fn composition_reports_rule_ownership_without_claiming_the_same_change_as_common_rules() {
    let source = base();
    let metadata = overlay(&source, vec![row("a", true, rule("a.example"))]);
    let app = scoped(&AppConfig::default(), "a", metadata.clone());
    let candidate = composition::finalize(
        &source,
        apply(&source, &metadata).unwrap(),
        &composition::Inputs {
            source_profile_id: Some("a".into()),
            app,
            supports_tolerance: false,
            selections: BTreeMap::new(),
        },
    )
    .unwrap();
    let layer = candidate
        .report
        .layers
        .iter()
        .find(|l| l.source == "profile_route_rules")
        .unwrap();
    assert_eq!(layer.changed_paths, vec!["/route/rules"]);
    let common = candidate
        .report
        .layers
        .iter()
        .find(|l| l.source == "common_rules")
        .unwrap();
    assert!(common.changed_paths.is_empty());
    let report = serde_json::to_string(&candidate.report).unwrap();
    assert!(!report.contains("a.example"));
    assert!(!report.contains("rows"));
}

#[cfg(unix)]
#[test]
fn native_dependency_gate_returns_before_opening_a_fifo_from_source() {
    use std::{ffi::CString, io::Write, os::unix::fs::OpenOptionsExt, sync::mpsc, time::Duration};
    let directory = tempfile::tempdir().unwrap();
    let fifo = directory.path().join("not-authorized-rule-material");
    let path = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: path is NUL-terminated and belongs to this test's private tmpdir.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    for geoip in [false, true] {
        let mut source = base();
        if geoip {
            source["route"]["geoip_database"] = json!(fifo.to_str().unwrap());
        } else {
            source["route"]["rule_sets"] = json!([{"tag":"file","type":"file","path":fifo.to_str().unwrap(),"format":"domain_list"}]);
        }
        let metadata = overlay(&source, vec![row("disabled", false, rule("a.example"))]);
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(apply(&source, &metadata));
        });
        let result = rx.recv_timeout(Duration::from_secs(2));
        if result.is_err() {
            // Release a regressed reader so the test fails rather than hanging.
            if let Ok(mut writer) = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
            {
                let _ = writer.write_all(b"example.invalid\n");
            }
        }
        let error = result
            .expect("route projection attempted FIFO IO")
            .unwrap_err();
        assert_eq!(
            error.details.as_ref().unwrap()["errors"][0]["reasonCode"],
            "dependency_authorization_required"
        );
        worker.join().unwrap();
    }
}
