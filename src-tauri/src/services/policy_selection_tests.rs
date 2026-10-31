use super::*;
use crate::models::{
    app_config::AppConfig, proxy_config::ProxyConfigProfile, subscription::SubscriptionProfile,
};
use serde_json::json;

fn profile(id: &str, active: bool) -> ProxyConfigProfile {
    // Old RC records have no profile-level policySelections field.
    serde_json::from_value(json!({
        "id": id, "name": id, "kernel": "zero", "format": "json", "active": active,
        "updatedAtUnixMs": 1, "capabilities": crate::models::proxy_config::ProxyConfigCapabilities::default(),
        "content": {
            "outbounds": [
                {"tag":"ix-01", "protocol":{"type":"direct"}},
                {"tag":"ix-02", "protocol":{"type":"direct"}}
            ],
            "outbound_groups":[{"tag":"AI Suite", "type":"selector", "outbounds":["ix-01", "ix-02"], "selected":"ix-01"}],
            "route":{"rules":[], "final":{"type":"route", "outbound":"AI Suite"}}
        }
    })).unwrap()
}

fn source(id: &str, target: &str) -> SubscriptionProfile {
    serde_json::from_value(json!({
        "id": id, "name": id, "url": format!("https://example.test/{id}"),
        "kernel":"zero", "format":"json", "enabled":true,
        "targetProxyConfigId":target, "updatedAtUnixMs":1,
        "policySelections":{"AI Suite":"ix-01"}
    }))
    .unwrap()
}

fn state(profiles: Vec<ProxyConfigProfile>, sources: Vec<SubscriptionProfile>) -> AppState {
    AppState::with_domain_data(AppConfig::default(), profiles, sources, vec![], vec![])
}

#[test]
fn local_choice_survives_sqlite_reload_and_profile_switch_without_changing_source() {
    let dir = std::env::temp_dir().join(format!(
        "znet-selector-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut profiles = vec![profile("local", true), profile("other", false)];
    let original = profiles[0].content.clone();
    let mut sources = vec![];
    assert!(record_selection(
        &mut profiles,
        &mut sources,
        "AI Suite",
        "ix-02",
        |profiles, sources| {
            crate::services::app_database::save_domain_data(&dir, profiles, sources)
        }
    )
    .unwrap());
    let mut loaded = crate::services::app_database::load_domain_data(&dir).unwrap();
    assert_eq!(loaded.proxy_configs[0].content, original);
    assert_eq!(
        loaded.proxy_configs[0].policy_selections["AI Suite"],
        "ix-02"
    );
    loaded.proxy_configs[0].active = false;
    loaded.proxy_configs[1].active = true;
    let other = state(loaded.proxy_configs.clone(), vec![]);
    assert_eq!(
        active_content(&other).unwrap().unwrap()["outbound_groups"][0]["selected"],
        "ix-01"
    );
    loaded.proxy_configs[0].active = true;
    loaded.proxy_configs[1].active = false;
    let restored = state(loaded.proxy_configs, vec![]);
    let fallback = active_content(&restored).unwrap().unwrap();
    let groups = crate::kernel::zero::config::policy_groups_from_config(&fallback);
    assert_eq!(groups[0].selected.as_deref(), Some("ix-02"));
    let base = original.unwrap();
    let effective = crate::services::rule_overlay::compose_effective_config_for(
        &restored,
        &base,
        Some("local"),
    )
    .unwrap();
    assert_eq!(effective["outbound_groups"][0]["selected"], "ix-02");
    assert_eq!(
        restored.proxy_configs().lock().unwrap()[0].content.as_ref(),
        Some(&base)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_profile_identity_keeps_choices_separate_for_identical_source_json() {
    let first = profile("first", true);
    let mut second = profile("second", false);
    second
        .policy_selections
        .insert("AI Suite".into(), "ix-02".into());
    let base = first.content.clone().unwrap();
    let state = state(vec![first, second], vec![]);
    let candidate =
        crate::services::rule_overlay::compose_effective_config_for(&state, &base, Some("second"))
            .unwrap();
    assert_eq!(candidate["outbound_groups"][0]["selected"], "ix-02");
    assert_eq!(
        active_content(&state).unwrap().unwrap()["outbound_groups"][0]["selected"],
        "ix-01"
    );
}

#[test]
fn subscription_choices_are_compatible_and_only_associated_sources_are_updated() {
    let mut profiles = vec![profile("local", true), profile("foreign", false)];
    let mut sources = vec![
        source("one", "local"),
        source("two", "local"),
        source("foreign", "foreign"),
    ];
    assert_eq!(
        resolve_selections(&profiles, &sources, "local")["AI Suite"],
        "ix-01"
    );
    record_selection(
        &mut profiles,
        &mut sources,
        "AI Suite",
        "ix-02",
        |next, sources| {
            assert_eq!(next[0].policy_selections["AI Suite"], "ix-02");
            assert_eq!(sources[0].policy_selections["AI Suite"], "ix-02");
            assert_eq!(sources[1].policy_selections["AI Suite"], "ix-02");
            assert_eq!(sources[2].policy_selections["AI Suite"], "ix-01");
            Ok(())
        },
    )
    .unwrap();
    sources[0]
        .policy_selections
        .insert("AI Suite".into(), "ix-01".into());
    assert_eq!(
        resolve_selections(&profiles, &sources, "local")["AI Suite"],
        "ix-02"
    );
}

#[test]
fn failed_storage_and_invalid_targets_do_not_publish_saved_choices() {
    let mut profiles = vec![profile("local", true)];
    let mut sources = vec![source("one", "local")];
    assert!(
        record_selection(&mut profiles, &mut sources, "AI Suite", "ix-02", |_, _| {
            Err(AppError::internal("disk unavailable"))
        })
        .is_err()
    );
    assert!(profiles[0].policy_selections.is_empty());
    assert_eq!(sources[0].policy_selections["AI Suite"], "ix-01");
    assert!(record_selection(
        &mut profiles,
        &mut sources,
        "AI Suite",
        "missing",
        |_, _| panic!("must not persist")
    )
    .is_err());
    assert!(profiles[0].policy_selections.is_empty());
}

#[test]
fn offline_node_browsing_includes_global_user_endpoints_without_modifying_profiles() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let original = profile("local", true);
    let mut app = AppConfig::default();
    app.user_endpoints
        .push(json!({"tag":"global-wg","protocol":{"type":"wireguard",
        "private_key":STANDARD.encode([1;32]),"addresses":["10.2.0.1/32"],
        "peers":[{"public_key":STANDARD.encode([2;32]),"allowed_ips":["0.0.0.0/0"]}]}}));
    let state = AppState::with_domain_data(app, vec![original.clone()], vec![], vec![], vec![]);
    let content = active_content(&state).unwrap().unwrap();
    let nodes = crate::kernel::zero::config::proxy_nodes_from_config(&content);
    assert!(nodes.iter().any(|node| node.tag == "global-wg"));
    assert_eq!(
        common::lock(state.proxy_configs(), "proxy_config").unwrap()[0].content,
        original.content
    );
}
