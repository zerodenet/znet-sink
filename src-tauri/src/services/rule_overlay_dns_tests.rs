use super::*;
use crate::models::proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile};

fn state(profiles: Vec<RuleSetProfile>, tag: &str, mode: &str) -> AppState {
    let mut app = AppConfig::default();
    app.client_edits = Some(std::collections::BTreeMap::from([(
        "dns".into(),
        json!({
            "enabled":true,"dnsHijack":true,"config":{
                "servers":{"system":{"type":"system"}},"default_server":"system",
                "dispatch":[{"condition":{"type":"and","conditions":[
                    {"type":"rule_set","tag":tag},{"type":"rule_set","tag":tag}
                ]},"server":"system"}]
            }
        }),
    )]));
    AppState::with_domain_data(
        app,
        vec![ProxyConfigProfile {
            id: "active".into(),
            name: "Active".into(),
            kernel: "zero".into(),
            format: "zero-json".into(),
            path: None,
            content: Some(json!({"mode":{"type":mode},"route":{
                "rule_sets":[],"rules":[],"final":{"type":"direct"}
            }})),
            active: true,
            policy_selections: Default::default(),
            managed_source: None,
            updated_at_unix_ms: 0,
            capabilities: ProxyConfigCapabilities::default(),
        }],
        vec![],
        profiles,
        vec![],
    )
}

#[test]
fn dns_assets_are_selectable_without_common_route_bindings_or_injection() {
    let (mut profile, path) = tests::verified_profile("dns-only", 0, CommonRuleAction::Reject);
    profile.common_binding = None;
    let tag = common_tag(&profile.id);
    let state = state(vec![profile], &tag, "direct");
    let options = effective_rule_set_options(&state).unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].tag, tag);
    assert_eq!(options[0].name, "dns-only");
    fs::remove_file(path).unwrap();
}

#[test]
fn dns_assets_materialize_once_in_all_modes_without_changing_route_actions() {
    let (mut profile, path) = tests::verified_profile("dns-modes", 0, CommonRuleAction::Reject);
    profile.common_binding = None;
    let tag = common_tag(&profile.id);
    for mode in ["rule", "global", "direct"] {
        let state = state(vec![profile.clone()], &tag, mode);
        let effective = current_effective_config(&state).unwrap().unwrap();
        assert_eq!(effective["route"]["rule_sets"].as_array().unwrap().len(), 1);
        assert_eq!(effective["route"]["rule_sets"][0]["tag"], tag);
        assert_eq!(effective["route"]["rules"], json!([]));
        assert_eq!(effective["route"]["final"], json!({"type":"direct"}));
        assert!(
            active_content(&state).unwrap().unwrap()["route"]["rule_sets"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn dns_draft_materializes_its_own_assets_and_does_not_keep_saved_dependencies() {
    let (mut profile, path) = tests::verified_profile("draft-dns", 0, CommonRuleAction::Direct);
    profile.common_binding = None;
    let tag = common_tag(&profile.id);
    let state = state(vec![profile], "gui-common-deleted", "rule");
    // A stale saved reference cannot block choosing a valid replacement.
    assert_eq!(effective_rule_set_options(&state).unwrap()[0].tag, tag);
    let base = active_content(&state).unwrap().unwrap();
    let draft = serde_json::from_value(json!({"enabled":true,"config":{
        "servers":{"system":{"type":"system"}},"default_server":"system",
        "dispatch":[{"condition":{"type":"rule_set","tag":tag},"server":"system"}]
    }}))
    .unwrap();
    let preview = compose_effective_config_with_dns(&state, &base, &draft).unwrap();
    assert_eq!(preview["route"]["rule_sets"][0]["tag"], tag);
    let cleared = serde_json::from_value(json!({"enabled":false})).unwrap();
    let preview = compose_effective_config_with_dns(&state, &base, &cleared).unwrap();
    assert_eq!(preview["route"]["rule_sets"], json!([]));
    fs::remove_file(path).unwrap();
}

#[test]
fn referenced_disabled_deleted_missing_or_corrupt_client_assets_reject_composition() {
    for case in ["disabled", "deleted", "missing", "corrupt"] {
        let (mut profile, path) = tests::verified_profile(case, 0, CommonRuleAction::Direct);
        profile.common_binding = None;
        let tag = common_tag(&profile.id);
        match case {
            "disabled" => profile.enabled = false,
            "missing" => fs::remove_file(&path).unwrap(),
            "corrupt" => fs::write(&path, b"corrupt").unwrap(),
            _ => {}
        }
        let profiles = if case == "deleted" {
            vec![]
        } else {
            vec![profile]
        };
        let state = state(profiles, &tag, "rule");
        assert!(current_effective_config(&state).is_err(), "{case}");
        if matches!(case, "disabled" | "deleted" | "missing") {
            assert!(effective_rule_set_options(&state).unwrap().is_empty());
        }
        let _ = fs::remove_file(path);
    }
}

#[test]
fn dns_assets_reuse_common_route_definitions_and_reject_tag_collisions() {
    let (profile, path) = tests::verified_profile("shared-dns", 0, CommonRuleAction::Direct);
    let tag = common_tag(&profile.id);
    let base = json!({"mode":{"type":"rule"},"route":{"rules":[],"rule_sets":[],"final":{"type":"direct"}}});
    let mut config = compose_with(&base, true, &[profile.clone()])
        .unwrap()
        .config;
    config["runtime"] =
        json!({"dns":{"dispatch":[{"condition":{"type":"rule_set","tag":tag},"server":"system"}]}});
    crate::configuration::dns_rule_sets::materialize(
        &mut config,
        &[profile.clone()],
        verify_artifact,
    )
    .unwrap();
    assert_eq!(config["route"]["rule_sets"].as_array().unwrap().len(), 1);
    assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 1);
    config["route"]["rule_sets"][0]["path"] = json!("/different-resource.zrs");
    assert!(crate::configuration::dns_rule_sets::materialize(
        &mut config,
        &[profile],
        verify_artifact
    )
    .is_err());
    fs::remove_file(path).unwrap();
}
