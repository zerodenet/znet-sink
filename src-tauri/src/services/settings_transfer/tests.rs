use super::*;
use serde_json::json;

fn doc(edits: Value) -> SettingsDocument {
    parse(&json!({"schemaVersion":SCHEMA,"clientEdits":edits}).to_string()).unwrap()
}
fn dns() -> Value {
    json!({"enabled":true,"dnsHijack":false,"config":{
        "servers":{"primary":{"type":"system"},"secondary":{"type":"udp","host":"1.1.1.1"}},
        "default_server":"primary",
        "policy":{"node_server":"primary","direct_server":"primary","fallback_servers":["secondary"],"node_fallback_servers":["secondary"],"direct_fallback_servers":["secondary"]}
    }})
}
fn prepare(state: &AppState, value: Value) -> TransferPreview {
    preview(
        state,
        "test-main",
        &json!({"schemaVersion":SCHEMA,"clientEdits":value}).to_string(),
    )
    .unwrap()
}

#[test]
fn unknown_envelope_and_forbidden_fields_are_rejected_before_candidate() {
    for value in [
        json!({"schemaVersion":SCHEMA,"permissions":{}}),
        json!({"schemaVersion":"future","preferences":{}}),
        json!({"schemaVersion":SCHEMA,"preferences":{"autoRecover":true}}),
    ] {
        assert!(parse(&value.to_string()).is_err());
    }
    for key in [
        "tun.enabled",
        "tun.name",
        "endpoints",
        "core.socket",
        "profileEdits",
        "plugins",
    ] {
        assert!(candidate(
            &AppConfig::default(),
            &doc(json!({key:true})),
            None,
            &json!({})
        )
        .is_err());
    }
}
#[test]
fn export_omits_secret_dns_and_urls_wholesale_without_leaking_metadata() {
    let secret = "never-export-this-token";
    let mut current = AppConfig::default();
    current.core.executable_path = Some(format!("/private/{secret}/zero"));
    current.ui.theme = secret.into();
    current.logs.level = secret.into();
    let mut unsafe_dns = dns();
    unsafe_dns["config"]["servers"]["primary"] = json!({"type":"doh","host":"dns.example","path":format!("/dns-query?token={secret}"),"bootstrap":["1.1.1.1"]});
    current.client_edits = Some(BTreeMap::from([
        ("dns".into(), unsafe_dns),
        (
            "urlTest.url".into(),
            json!(format!("https://example.com/generate_204?token={secret}")),
        ),
        (format!("unknown-{secret}"), json!(secret)),
        ("tun.mtu".into(), json!(1400)),
    ]));
    let exported = export(&current).unwrap();
    let all = serde_json::to_string(&exported).unwrap();
    assert!(!all.contains(secret));
    let exported = parse(&exported.document).unwrap();
    assert!(!exported.client_edits.contains_key("dns"));
    assert!(!exported.client_edits.contains_key("urlTest.url"));
    assert_eq!(exported.client_edits.get("tun.mtu"), Some(&json!(1400)));
}
#[test]
fn all_dns_reference_categories_and_native_unknown_fields_are_rejected() {
    for field in [
        "fallback_servers",
        "node_fallback_servers",
        "direct_fallback_servers",
    ] {
        let mut value = dns();
        value["config"]["policy"][field] = json!(["missing"]);
        assert!(
            candidate(
                &AppConfig::default(),
                &doc(json!({"dns":value})),
                None,
                &json!({})
            )
            .is_err(),
            "{field}"
        );
    }
    for field in ["node_server", "direct_server"] {
        let mut value = dns();
        value["config"]["policy"][field] = json!("missing");
        assert!(
            candidate(
                &AppConfig::default(),
                &doc(json!({"dns":value})),
                None,
                &json!({})
            )
            .is_err(),
            "{field}"
        );
    }
    let mut value = dns();
    value["config"]["dispatch"] =
        json!([{"condition":{"type":"domain","values":["example.com"]},"server":"missing"}]);
    assert!(candidate(
        &AppConfig::default(),
        &doc(json!({"dns":value})),
        None,
        &json!({})
    )
    .is_err());
    let mut value = dns();
    value["config"]["servers"]["primary"]["token"] = json!("secret");
    assert!(candidate(
        &AppConfig::default(),
        &doc(json!({"dns":value})),
        None,
        &json!({})
    )
    .is_err());
    assert!(candidate(
        &AppConfig::default(),
        &doc(json!({"dns":dns()})),
        None,
        &json!({})
    )
    .is_ok());
}
#[test]
fn numeric_types_cidr_families_and_ranges_are_validated_together() {
    for edits in [
        json!({"tun.mtu":575}),
        json!({"tun.mtu":65536}),
        json!({"localProxy.port":0}),
        json!({"runtime.udpUpstreamIdleTimeoutSeconds":0}),
        json!({"tun.dualStack":"true"}),
        json!({"tun.addr":"10.0.0.1/99"}),
        json!({"tun.dualStack":false,"tun.secondaryAddr":"fd00::1/64"}),
    ] {
        assert!(candidate(&AppConfig::default(), &doc(edits), None, &json!({})).is_err());
    }
}
#[test]
fn omitted_preferences_and_excluded_ownership_survive_offline_save_candidate() {
    let mut current = AppConfig::default();
    current.core.executable_path = Some("/device/private/zero".into());
    current.tun.enabled = Some(true);
    current.profile_edits.insert(
        "original".into(),
        BTreeMap::from([("endpoints".into(), json!({"device":"private-preference"}))]),
    );
    let next = candidate(
        &current,
        &doc(json!({"tun.mtu":1400,"localProxy.port":9876})),
        None,
        &json!({}),
    )
    .unwrap();
    assert_eq!(next.core, current.core);
    assert_eq!(next.tun.enabled, current.tun.enabled);
    assert_eq!(next.profile_edits, current.profile_edits);
    assert_eq!(
        next.client_edits.as_ref().unwrap()["localProxy.port"],
        json!(9876)
    );
}
#[test]
fn preview_is_readonly_commit_is_atomic_idempotent_and_records_pending_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let state = AppState::new(AppConfig::default());
    let before = lock(state.app_config(), "test").unwrap().clone();
    let preview = prepare(&state, json!({"tun.mtu":1400}));
    assert_eq!(*lock(state.app_config(), "test").unwrap(), before);
    assert!(!path.exists());
    let receipt = commit_with(&state, "test-main", &preview.plan_id, |expected, next| {
        app_config::compare_and_replace_local_at(&state, expected, next, &path)
    })
    .unwrap();
    assert_eq!(receipt.local_state, "saved");
    assert_eq!(receipt.runtime_state, "pending-apply");
    assert_eq!(
        crate::services::app_config_store::load_or_default(&path).unwrap(),
        *lock(state.app_config(), "test").unwrap()
    );
    let again = commit_with(&state, "test-main", &preview.plan_id, |_, _| {
        panic!("must not replay publication")
    })
    .unwrap();
    assert_eq!(again.saved_revision, receipt.saved_revision);
    assert_eq!(
        status(&state, "test-main", &preview.plan_id)
            .unwrap()
            .local_state,
        "saved"
    );
}
#[test]
fn changed_settings_owner_or_scope_reject_before_publication() {
    let state = AppState::new(AppConfig::default());
    let preview = prepare(&state, json!({"tun.mtu":1400}));
    assert!(
        commit_with(&state, "other-window", &preview.plan_id, |_, _| panic!(
            "owner rejection before write"
        ))
        .is_err()
    );
    lock(state.app_config(), "test").unwrap().ui.theme = "dark".into();
    let path = tempfile::tempdir().unwrap();
    assert!(
        commit_with(&state, "test-main", &preview.plan_id, |expected, next| {
            app_config::compare_and_replace_local_at(
                &state,
                expected,
                next,
                &path.path().join("config"),
            )
        })
        .is_err()
    );
    assert!(!path.path().join("config").exists());
    assert_eq!(
        status(&state, "test-main", &preview.plan_id)
            .unwrap()
            .local_state,
        "unknown"
    );
}
#[test]
fn failed_atomic_storage_leaves_settings_and_disk_unchanged_and_never_rolls_back_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let state = AppState::new(AppConfig::default());
    let before = lock(state.app_config(), "test").unwrap().clone();
    crate::services::app_config_store::save(&path, &before).unwrap();
    std::fs::remove_file(path.with_file_name("settings.json.bak")).unwrap();
    std::fs::create_dir(path.with_file_name("settings.json.bak")).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let preview = prepare(&state, json!({"tun.mtu":1400}));
    assert!(
        commit_with(&state, "test-main", &preview.plan_id, |expected, next| {
            app_config::compare_and_replace_local_at(&state, expected, next, &path)
        })
        .is_err()
    );
    assert_eq!(*lock(state.app_config(), "test").unwrap(), before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        status(&state, "test-main", &preview.plan_id)
            .unwrap()
            .local_state,
        "not-committed"
    );
}
#[test]
fn preference_only_plan_has_no_runtime_application_claim() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::new(AppConfig::default());
    let preview = preview(
        &state,
        "test-main",
        &json!({"schemaVersion":SCHEMA,"preferences":{"theme":"dark"}}).to_string(),
    )
    .unwrap();
    assert!(!preview.runtime_required);
    let receipt = commit_with(&state, "test-main", &preview.plan_id, |expected, next| {
        app_config::compare_and_replace_local_at(
            &state,
            expected,
            next,
            &directory.path().join("settings"),
        )
    })
    .unwrap();
    assert_eq!(receipt.runtime_state, "not-required");
    assert_eq!(receipt.theme, "dark");
}

#[test]
fn preference_only_import_does_not_migrate_legacy_network_choices() {
    let mut current = AppConfig::default();
    current.client_edits = None;
    current.overrides.dns = true;
    let mut document = doc(json!({}));
    document.preferences.theme = Some("dark".into());
    let next = candidate(&current, &document, None, &json!({})).unwrap();
    assert_eq!(next.client_edits, current.client_edits);
    assert_eq!(next.overrides, current.overrides);
    assert_eq!(next.dns, current.dns);
    assert_eq!(next.ui.theme, "dark");
}

#[test]
fn expired_preview_cannot_publish_after_confirmation() {
    let state = AppState::new(AppConfig::default());
    let preview = prepare(&state, json!({"tun.mtu":1400}));
    lock(plans(), "test")
        .unwrap()
        .get_mut(&preview.plan_id)
        .unwrap()
        .expires = 0;
    assert!(
        commit_with(&state, "test-main", &preview.plan_id, |_, _| panic!(
            "expired plan must not write"
        ))
        .is_err()
    );
}

#[test]
fn dns_fake_ip_cannot_overlap_the_combined_tun_owned_addresses() {
    let mut value = dns();
    value["config"]["answer"] = json!({"type":"fake_ip","cidr":"10.66.0.0/24"});
    assert!(candidate(
        &AppConfig::default(),
        &doc(json!({"dns":value})),
        None,
        &json!({})
    )
    .is_err());
}
#[test]
fn shared_normalization_is_stored_in_the_sparse_candidate() {
    let next = candidate(
        &AppConfig::default(),
        &doc(json!({"tun.addr":" 10.0.0.1/24 ","tun.includeCidrs":[" 0.0.0.0/0 ","0.0.0.0/0"]})),
        None,
        &json!({}),
    )
    .unwrap();
    let edits = next.client_edits.unwrap();
    assert_eq!(edits["tun.addr"], json!("10.0.0.1/24"));
    assert_eq!(edits["tun.includeCidrs"], json!(["0.0.0.0/0"]));
}

#[test]
fn source_metadata_version_change_rejects_an_otherwise_matching_scope() {
    let profile = crate::models::proxy_config::ProxyConfigProfile {
        policy_selections: Default::default(),
        id: "profile-a".into(),
        name: "A".into(),
        kernel: "zero".into(),
        format: "zero".into(),
        path: None,
        content: Some(json!({})),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: Default::default(),
    };
    let state =
        AppState::with_domain_data(AppConfig::default(), vec![profile], vec![], vec![], vec![]);
    let preview = preview(
        &state,
        "test-main",
        &json!({"schemaVersion":SCHEMA,"preferences":{"theme":"dark"}}).to_string(),
    )
    .unwrap();
    assert_eq!(preview.source_revision.as_deref(), Some("1"));
    lock(state.proxy_configs(), "test").unwrap()[0].updated_at_unix_ms = 2;
    assert!(
        commit_with(&state, "test-main", &preview.plan_id, |_, _| panic!(
            "changed source version must not publish"
        ))
        .is_err()
    );
}

#[test]
fn private_dns_hostnames_and_sni_are_not_guessed_safe_or_exported() {
    let mut current = AppConfig::default();
    for server in [
        json!({"type":"doh","host":"opaque-auth.private-gateway.example","path":"/dns-query","bootstrap":["1.1.1.1"]}),
        json!({"type":"dot","host":"1.1.1.1","server_name":"opaque-auth.private-gateway.example"}),
        json!({"type":"doh","host":"dns.google","server_name":"cloudflare-dns.com","path":"/dns-query","bootstrap":["8.8.8.8"]}),
    ] {
        let mut value = dns();
        value["config"]["servers"]["primary"] = server;
        assert!(candidate(&current, &doc(json!({"dns":value})), None, &json!({})).is_err());
        current.client_edits = Some(BTreeMap::from([("dns".into(), value.clone())]));
        let before = current.clone();
        let exported = export(&current).unwrap();
        assert!(exported.omitted_fields.contains(&"dns".into()));
        assert!(!exported.document.contains("opaque-auth"));
        assert_eq!(current, before);
    }
    let recommended = serde_json::to_value(AppConfig::default().dns).unwrap();
    assert!(validation::validate_field("dns", &recommended, &AppConfig::default()).is_ok());
}

#[test]
fn disabled_dns_and_unit_variants_reject_ignored_native_fields() {
    for field in ["server", "answer", "matcher"] {
        let mut value = dns();
        value["enabled"] = json!(false);
        match field {
            "server" => value["config"]["servers"]["primary"]["unexpected"] = json!(null),
            "answer" => value["config"]["answer"] = json!({"type":"real","unexpected":"secret"}),
            _ => {
                value["config"]["dispatch"] = json!([{"condition":{"type":"domain","values":["example.com"],"unexpected":"secret"},"server":"primary"}])
            }
        }
        assert!(
            candidate(
                &AppConfig::default(),
                &doc(json!({"dns":value})),
                None,
                &json!({})
            )
            .is_err(),
            "{field}"
        );
    }
}

#[test]
fn urltest_opaque_paths_are_rejected_and_omitted_without_changing_current_settings() {
    let mut current = AppConfig::default();
    for url in [
        "https://example.com/opaque-auth-token",
        "https://example.com/favicon.ico",
        "https://example.com/generate_204?token=secret",
    ] {
        assert!(candidate(&current, &doc(json!({"urlTest.url":url})), None, &json!({})).is_err());
        current.client_edits = Some(BTreeMap::from([("urlTest.url".into(), json!(url))]));
        let before = current.clone();
        let exported = export(&current).unwrap();
        assert!(exported.omitted_fields.contains(&"urlTest.url".into()));
        assert_eq!(current, before);
    }
    assert!(validation::validate_field(
        "urlTest.url",
        &json!(crate::models::app_config::default_url_test_url()),
        &current
    )
    .is_ok());
}
