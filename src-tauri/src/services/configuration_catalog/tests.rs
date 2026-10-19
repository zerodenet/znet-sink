use super::*;
use serde_json::json;

fn profile(id: &str) -> ProxyConfigProfile {
    serde_json::from_value(json!({"id":id,"name":"same name","kernel":"zero","format":"json",
        "active":true,"updatedAtUnixMs":1,"content":{"outbounds":[{"tag":"saved"}]},"capabilities":crate::models::proxy_config::ProxyConfigCapabilities::default()})).unwrap()
}
fn source(id: &str, target: Option<&str>) -> SubscriptionProfile {
    serde_json::from_value(
        json!({"id":id,"name":"same name","url":"https://source.test/sub",
        "kernel":"zero","format":"auto","enabled":true,"updatedAtUnixMs":2,
        "targetProxyConfigId":target,"policySelections":{"Proxy":"selected-leaf"},"usedBytes":375}),
    )
    .unwrap()
}
#[test]
fn linked_sources_fold_into_one_configuration_without_rewriting_ids_or_choices() {
    let profiles = vec![profile("original")];
    let sources = vec![
        source("first", Some("original")),
        source("second", Some("original")),
    ];
    let before = serde_json::to_value((&profiles, &sources)).unwrap();
    let entries = project(&profiles, &sources);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].profile_id.as_deref(), Some("original"));
    assert!(entries[0].active && entries[0].has_content);
    assert_eq!(
        serde_json::to_value(&entries[0].sources).unwrap(),
        serde_json::to_value(&sources).unwrap()
    );
    assert_eq!(serde_json::to_value((&profiles, &sources)).unwrap(), before);
    // List IPC never copies potentially megabytes of source JSON.
    assert!(serde_json::to_value(entries).unwrap()[0]
        .get("content")
        .is_none());
}
#[test]
fn unsynced_and_missing_targets_remain_visible_without_name_matching() {
    let entries = project(
        &[profile("local")],
        &[source("pending", None), source("lost", Some("missing"))],
    );
    assert_eq!(entries.len(), 3);
    assert!(entries[0].sources.is_empty());
    assert_eq!(entries[1].id, "subscription:pending");
    assert!(!entries[1].missing_profile);
    assert!(entries[2].missing_profile);
    assert!(!entries[1].active && !entries[1].has_content);
}
#[test]
fn plugin_namespace_revision_and_provider_quota_survive_projection() {
    let owner = ManagedSubscriptionSource {
        plugin_id: "plugin.test".into(),
        provider_id: "provider".into(),
        remote_subscription_id: "remote".into(),
        revision: Some("r7".into()),
        source_name: Some("source".into()),
    };
    let mut config = profile("owned");
    config.managed_source = Some(owner.clone());
    let mut sub = source("managed", Some("owned"));
    sub.managed_source = Some(owner.clone());
    let entries = project(&[config], &[sub]);
    assert_eq!(
        serde_json::to_value(&entries[0].managed_source).unwrap(),
        serde_json::to_value(owner).unwrap()
    );
    assert_eq!(entries[0].sources[0].used_bytes, Some(375));
    assert_eq!(
        entries[0].sources[0].policy_selections["Proxy"],
        "selected-leaf"
    );
}
#[test]
fn deleting_a_source_preserves_the_existing_configuration_identity() {
    let config = profile("kept");
    let before = project(
        std::slice::from_ref(&config),
        &[source("removed", Some("kept"))],
    );
    let after = project(&[config], &[]);
    assert_eq!(before[0].id, after[0].id);
    assert_eq!(before[0].profile_id, after[0].profile_id);
    assert!(after[0].active && after[0].has_content);
}

#[test]
fn equal_content_configurations_keep_independent_choices_during_composition() {
    let mut first = profile("first");
    let mut second = first.clone();
    second.id = "second".into();
    second.active = false;
    first
        .policy_selections
        .insert("Proxy".into(), "node-a".into());
    second
        .policy_selections
        .insert("Proxy".into(), "node-b".into());
    let state = AppState::with_domain_data(
        Default::default(),
        vec![first, second],
        vec![],
        vec![],
        vec![],
    );
    let first_choices =
        crate::services::policy_selection::selections_for_profile(&state, "first").unwrap();
    let second_choices =
        crate::services::policy_selection::selections_for_profile(&state, "second").unwrap();
    assert_eq!(first_choices["Proxy"], "node-a");
    assert_eq!(second_choices["Proxy"], "node-b");
}

#[test]
fn first_publication_inherits_unlinked_choices_without_consuming_the_source() {
    let mut source = source("pending", None);
    let original_choices = source.policy_selections.clone();
    let mut profiles = vec![profile("generated")];
    assert!(
        !crate::services::policy_selection::inherit_legacy_selections(
            &mut profiles,
            &[source.clone()]
        )
    );
    source.target_proxy_config_id = Some("generated".into());
    assert!(
        crate::services::policy_selection::inherit_legacy_selections(
            &mut profiles,
            &[source.clone()]
        )
    );
    assert_eq!(profiles[0].policy_selections, original_choices);
    assert_eq!(source.policy_selections, original_choices);
    assert!(
        !crate::services::policy_selection::inherit_legacy_selections(&mut profiles, &[source])
    );
}
