use super::*;
use crate::models::{
    proxy_config::ProxyConfigProfile,
    subscription::{ManagedSubscriptionSource, SubscriptionProfile},
};
fn base() -> Value {
    json!({"mode":{"type":"rule"},"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],
        "route":{"rules":[{"condition":{"type":"domain","values":["source.test"]},"action":{"type":"reject"}}],"final":{"type":"direct"}},
        "runtime":{"log":{"level":"info","files":[{"path":"never-created.log","max_bytes":u64::MAX}]}}})
}
fn profile(source: &Value) -> ProxyConfigProfile {
    serde_json::from_value(json!({"id":"ordinary-route","name":"ordinary","kernel":"zero","format":"json","path":null,
        "content":source,"active":true,"updatedAtUnixMs":1,"capabilities":{"hasProxyNodes":false,"hasProxyGroups":false,"hasRouteRules":true,"hasRuleSets":false,"hasSelector":false,"hasUrlTest":false,"featureKeys":[]}})).unwrap()
}
fn app(source: &Value) -> AppConfig {
    let mut app = AppConfig::default();
    app.profile_edits.entry("ordinary-route".into()).or_default().insert("routeRules".into(),json!({"version":1,
        "sourceDigest":crate::configuration::route_rules::source_digest(source).unwrap(),
        "rows":[{"id":"edited","enabled":true,"rule":{"condition":{"type":"domain","values":["edit.test"]},"action":{"type":"direct"},"mode":"flow"}},
        {"id":"disabled-source","enabled":false,"rule":source["route"]["rules"][0]}]}));
    app
}
fn managed() -> ManagedSubscriptionSource {
    ManagedSubscriptionSource {
        plugin_id: "plugin".into(),
        provider_id: "component".into(),
        remote_subscription_id: "resource".into(),
        revision: None,
        source_name: None,
    }
}
#[test]
fn actual_and_dns_preview_composition_project_once_before_verified_public_rules() {
    let source = base();
    let mut app = app(&source);
    app.routing.inject_common_rules = true;
    app.client_edits
        .as_mut()
        .unwrap()
        .insert("routing.injectCommonRules".into(), json!(true));
    let (public, path) =
        super::tests::verified_profile("route-editor-public", 0, CommonRuleAction::Direct);
    let state = AppState::with_domain_data(
        app.clone(),
        vec![profile(&source)],
        vec![],
        vec![public],
        vec![],
    );
    let candidate =
        compose_effective_candidate_with_app(&state, &source, Some("ordinary-route"), &app)
            .unwrap();
    for effective in [
        candidate.config,
        compose_effective_config_with_dns(&state, &source, &app.dns).unwrap(),
    ] {
        let rules = effective["route"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["condition"]["values"][0], "edit.test");
        assert_eq!(rules[0]["mode"], "flow");
        assert_eq!(
            rules[1]["condition"]["tag"],
            common_tag("route-editor-public")
        );
        assert_eq!(
            effective["runtime"]["log"]["files"][0]["max_bytes"].as_u64(),
            Some(u64::MAX)
        );
        assert!(rules
            .iter()
            .all(|r| r.get("id").is_none() && r.get("enabled").is_none()));
    }
    assert_eq!(
        state.proxy_configs().lock().unwrap()[0].content.as_ref(),
        Some(&source)
    );
    fs::remove_file(path).unwrap();
}
#[test]
fn managed_same_id_with_identical_source_cannot_inherit_ordinary_route_override() {
    let source = base();
    let app = app(&source);
    let mut p = profile(&source);
    p.managed_source = Some(managed());
    let state = AppState::with_domain_data(app.clone(), vec![p], vec![], vec![], vec![]);
    let error =
        match compose_effective_candidate_with_app(&state, &source, Some("ordinary-route"), &app) {
            Ok(_) => panic!("managed source must reject"),
            Err(e) => e,
        };
    assert_eq!(error.code, "route_editor_read_only");
    assert!(error.details.is_none());
    assert!(compose_effective_config_with_dns(&state, &source, &app.dns).is_err());
}
#[test]
fn ordinary_subscription_can_use_overlay_but_managed_relationship_cannot() {
    let source = base();
    let app = app(&source);
    let mut subscription:SubscriptionProfile=serde_json::from_value(json!({"id":"subscription","name":"ordinary","url":"https://example.test/private?token=canary","enabled":true,
        "kernel":"zero","format":"json","targetProxyConfigId":"ordinary-route","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap();
    let state = AppState::with_domain_data(
        app.clone(),
        vec![profile(&source)],
        vec![subscription.clone()],
        vec![],
        vec![],
    );
    assert!(
        compose_effective_candidate_with_app(&state, &source, Some("ordinary-route"), &app).is_ok()
    );
    subscription.managed_source = Some(managed());
    *state.subscriptions().lock().unwrap() = vec![subscription];
    let error =
        match compose_effective_candidate_with_app(&state, &source, Some("ordinary-route"), &app) {
            Ok(_) => panic!("managed relationship must reject"),
            Err(e) => e,
        };
    assert_eq!(error.code, "route_editor_read_only");
    assert!(!error.message.contains("canary"));
}
