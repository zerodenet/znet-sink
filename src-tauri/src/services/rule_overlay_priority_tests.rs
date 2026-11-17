use super::*;
use crate::models::proxy_config::ProxyConfigProfile;
use std::collections::BTreeMap;

fn source() -> Value {
    json!({"mode":{"type":"rule"},"route":{
        "rules":[{"condition":{"type":"domain","values":["hhhh.dev"]},
            "action":{"type":"route","outbound":"proxy"}}],
        "final":{"type":"route","outbound":"proxy"},"bypass":[]},
        "outbounds":[{"tag":"proxy","protocol":{"type":"direct"}}]})
}

fn state(base: Value, profiles: Vec<RuleSetProfile>, setting: Option<bool>) -> AppState {
    let mut app = AppConfig::default();
    app.dns.enabled = false;
    app.client_edits = Some(BTreeMap::new());
    if let Some(enabled) = setting {
        app.client_edits
            .as_mut()
            .unwrap()
            .insert("routing.injectCommonRules".into(), json!(enabled));
    }
    AppState::with_domain_data(
        app,
        vec![ProxyConfigProfile {
            id: "active".into(),
            name: "Active".into(),
            kernel: "zero".into(),
            format: "zero-json".into(),
            path: None,
            content: Some(base),
            active: true,
            policy_selections: Default::default(),
            managed_source: None,
            updated_at_unix_ms: 0,
            capabilities: Default::default(),
        }],
        vec![],
        profiles,
        vec![],
    )
}

#[test]
fn enabled_custom_binding_overrides_inherited_rules_on_startup_and_hot_recompose() {
    let (profile, path) = tests::verified_profile_with_rules(
        "custom-direct",
        1,
        CommonRuleAction::Direct,
        json!([{"type":"domain_suffix","value":"hhhh.dev"}]),
    );
    let mapped = zero_rule::zrs::MappedRuleSet::open(&path, VerifyMode::FullChecksum).unwrap();
    assert!(mapped.matches(&zero_rule::PreparedRuleQuery::new(Some("c.hhhh.dev"), None).unwrap()));
    assert!(!mapped.matches(&zero_rule::PreparedRuleQuery::new(Some("other.test"), None).unwrap()));
    drop(mapped);
    let base = source();
    let state = state(base.clone(), vec![profile.clone()], None);
    let startup = current_effective_config(&state).unwrap().unwrap();
    assert_eq!(
        startup["route"]["rules"][0]["condition"]["tag"],
        common_tag(&profile.id)
    );
    assert_eq!(
        startup["route"]["rules"][0]["action"],
        json!({"type":"direct"})
    );
    assert_eq!(startup["route"]["rules"][1], base["route"]["rules"][0]);
    assert_eq!(active_content(&state).unwrap().unwrap(), base);
    let hot = compose_effective_with(&state, &base, true, &[profile], None, None).unwrap();
    assert_eq!(hot, startup);
    let status = status(&state).unwrap();
    assert!(status.effective);
    assert_eq!(status.injected_count, 1);
    assert!(status.reason.is_none());
    fs::remove_file(path).unwrap();
}

#[test]
fn builtin_fallback_requires_opt_in_while_custom_binding_has_priority() {
    let (custom, custom_path) = tests::verified_profile("custom", 20, CommonRuleAction::Direct);
    let (mut builtin, builtin_path) =
        tests::verified_profile("builtin", 1, CommonRuleAction::Reject);
    builtin.built_in = true;
    for setting in [None, Some(true), Some(false)] {
        let base = source();
        let state = state(base.clone(), vec![builtin.clone(), custom.clone()], setting);
        let config = current_effective_config(&state).unwrap().unwrap();
        let rules = config["route"]["rules"].as_array().unwrap();
        let expected = match setting {
            None => 1,
            Some(true) => 2,
            Some(false) => 0,
        };
        assert_eq!(rules.len(), 1 + expected);
        let observed = status(&state).unwrap();
        assert_eq!(observed.injected_count, expected);
        assert_eq!(observed.effective, expected > 0);
        if expected > 0 {
            assert_eq!(rules[0]["condition"]["tag"], common_tag(&custom.id));
            assert_eq!(rules[1], base["route"]["rules"][0]);
        }
        if expected == 2 {
            assert_eq!(rules[2]["condition"]["tag"], common_tag(&builtin.id));
        }
        assert_eq!(active_content(&state).unwrap().unwrap(), base);
    }
    fs::remove_file(custom_path).unwrap();
    fs::remove_file(builtin_path).unwrap();
}

#[test]
fn disabling_a_custom_binding_removes_its_route_and_keeps_the_source_policy() {
    let (mut custom, path) =
        tests::verified_profile("disabled-custom", 1, CommonRuleAction::Direct);
    custom.common_binding.as_mut().unwrap().enabled = false;
    let base = source();
    let state = state(base.clone(), vec![custom], None);
    let effective = current_effective_config(&state).unwrap().unwrap();
    assert_eq!(effective["route"], base["route"]);
    assert!(!status(&state).unwrap().effective);
    fs::remove_file(path).unwrap();
}

#[test]
fn invalid_custom_artifact_rejects_activation_even_when_source_rules_are_inherited() {
    let (custom, path) = tests::verified_profile("corrupt-custom", 1, CommonRuleAction::Direct);
    fs::write(&path, b"invalid rule set").unwrap();
    let state = state(source(), vec![custom], None);
    assert!(current_effective_config(&state).is_err());
    let app = common::lock(state.app_config(), "app_config")
        .unwrap()
        .clone();
    assert!(validate_app_config_candidate(&state, &app).is_err());
    fs::remove_file(path).unwrap();
}

#[test]
fn custom_bindings_do_not_change_global_or_direct_mode_policy() {
    let (custom, path) = tests::verified_profile("rule-only", 1, CommonRuleAction::Direct);
    for mode in [
        json!({"type":"direct"}),
        json!({"type":"global","outbound":"proxy"}),
    ] {
        let mut base = source();
        base["mode"] = mode;
        let state = state(base.clone(), vec![custom.clone()], None);
        assert_eq!(
            current_effective_config(&state).unwrap().unwrap()["route"]["rules"],
            base["route"]["rules"]
        );
        assert!(!status(&state).unwrap().effective);
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn endpoint_domain_and_database_bindings_survive_recompose_without_public_injection() {
    let (domain, path) = tests::verified_profile_with_rules(
        "company-domain",
        0,
        CommonRuleAction::Endpoint("Starters".into()),
        json!([{"type":"domain_suffix","value":"starmerx.com"}]),
    );
    let (database, dbpath) = tests::verified_profile_with_rules(
        "company-db",
        1,
        CommonRuleAction::Endpoint("Starters".into()),
        json!([{"type":"ipv4_cidr","value":"192.168.1.235/32"}]),
    );
    let db = zero_rule::zrs::MappedRuleSet::open(&dbpath, VerifyMode::FullChecksum).unwrap();
    assert!(db.matches(
        &zero_rule::PreparedRuleQuery::new(None, Some("192.168.1.235".parse().unwrap())).unwrap()
    ));
    assert!(!db.matches(
        &zero_rule::PreparedRuleQuery::new(None, Some("192.168.1.234".parse().unwrap())).unwrap()
    ));
    drop(db);
    let mapped = zero_rule::zrs::MappedRuleSet::open(&path, VerifyMode::FullChecksum).unwrap();
    for name in ["starmerx.com", "vending.test.starmerx.com"] {
        assert!(mapped.matches(&zero_rule::PreparedRuleQuery::new(Some(name), None).unwrap()));
    }
    assert!(
        !mapped.matches(&zero_rule::PreparedRuleQuery::new(Some("notstarmerx.com"), None).unwrap())
    );
    drop(mapped);
    let mut base = source();
    base["endpoints"] = json!([{"tag":"Starters","enabled":true,"directions":{"outbound":true},"protocol":{"type":"wireguard","peers":[{"allowed_ips":["192.168.0.0/23"]}]}}]);
    let state = state(
        base.clone(),
        vec![domain.clone(), database.clone()],
        Some(false),
    );
    let candidate = current_effective_config(&state).unwrap().unwrap();
    assert_eq!(
        candidate["route"]["rules"][0]["action"],
        json!({"type":"route","outbound":"Starters"})
    );
    assert_eq!(
        candidate["route"]["rules"][1]["condition"]["tag"],
        common_tag(&database.id)
    );
    assert_eq!(candidate["route"]["rules"][2], base["route"]["rules"][0]);
    assert_eq!(candidate["route"]["rules"][0]["mode"], "translate");
    assert_eq!(
        current_effective_config(&state).unwrap().unwrap(),
        candidate
    );
    assert_eq!(active_content(&state).unwrap().unwrap(), base);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(dbpath).unwrap();
}

#[test]
fn profile_scoped_endpoint_binding_does_not_break_other_configurations() {
    let (mut profile, path) = tests::verified_profile_with_rules(
        "scoped-endpoint",
        0,
        CommonRuleAction::Endpoint("OnlyInOtherProfile".into()),
        json!([{"type":"domain_suffix","value":"starmerx.com"}]),
    );
    profile.common_binding.as_mut().unwrap().profile_id = Some("another-profile".into());
    let base = source();
    let state = state(base.clone(), vec![profile.clone()], Some(false));
    let candidate = current_effective_config(&state).unwrap().unwrap();
    assert_eq!(candidate["route"]["rules"], base["route"]["rules"]);
    assert!(!candidate.to_string().contains("OnlyInOtherProfile"));
    // Returning to the bound configuration still validates the real endpoint.
    assert!(compose_rules_for(&base, false, &[profile], false, Some("another-profile")).is_err());
    std::fs::remove_file(path).unwrap();
}
