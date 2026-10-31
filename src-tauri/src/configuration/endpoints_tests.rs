use super::*;
use serde_json::json;
fn source() -> Value {
    json!({"endpoints":[{"tag":"a","protocol":{"type":"future","key":"untouched"}}]})
}
fn row() -> Value {
    json!({"tag":"a","protocol":"future"})
}
fn state(enabled: bool) -> Action {
    Action::SetState {
        enabled,
        persistence: crate::services::network_endpoint::Persistence::RuntimeOnly,
    }
}

#[test]
fn endpoint_preferences_merge_fields_survive_restart_and_subscription_refresh() {
    let base = source();
    let app = candidate(
        &AppConfig::default(),
        "profile-a",
        &base,
        &row(),
        &state(false),
    )
    .unwrap();
    let app = candidate(
        &app,
        "profile-a",
        &base,
        &row(),
        &Action::SetDirections {
            directions: Directions {
                inbound: true,
                outbound: false,
            },
            persistence: crate::services::network_endpoint::Persistence::RuntimeOnly,
        },
    )
    .unwrap();
    let restored: AppConfig = serde_json::from_value(serde_json::to_value(app).unwrap()).unwrap();
    let mut refreshed = source();
    refreshed["endpoints"][0]["protocol"]["key"] = json!("updated-subscription-key");
    let (_, effective) =
        crate::configuration::local_edits::resolve(&restored, Some("profile-a"), &refreshed)
            .unwrap();
    assert_eq!(effective["endpoints"][0]["enabled"], false);
    assert_eq!(
        effective["endpoints"][0]["directions"],
        json!({"inbound":true,"outbound":false})
    );
    assert_eq!(
        effective["endpoints"][0]["protocol"]["key"],
        "updated-subscription-key"
    );
    assert_eq!(base, source());
    assert_eq!(
        crate::configuration::local_edits::resolve(&restored, Some("profile-b"), &refreshed)
            .unwrap()
            .1,
        refreshed
    );
    let app = candidate(&restored, "profile-a", &base, &row(), &state(true)).unwrap();
    let effective = crate::configuration::local_edits::resolve(&app, Some("profile-a"), &base)
        .unwrap()
        .1;
    assert_eq!(effective["endpoints"][0]["directions"]["inbound"], true);
}
#[test]
fn endpoint_preferences_do_not_create_missing_endpoints_or_reuse_other_protocol_tags() {
    let app = candidate(&AppConfig::default(), "a", &source(), &row(), &state(false)).unwrap();
    let prefs = &app.profile_edits["a"]["endpoints"];
    let mut missing = json!({});
    apply(&mut missing, prefs).unwrap();
    assert_eq!(missing, json!({}));
    let mut changed = source();
    changed["endpoints"][0]["protocol"]["type"] = json!("another");
    let previous = changed.clone();
    apply(&mut changed, prefs).unwrap();
    assert_eq!(changed, previous);
    assert!(!owns(&changed, &row()));
    assert!(candidate(&app, "a", &changed, &row(), &state(true)).is_err());
    assert!(validate(
        &json!({"a":{"protocol":"future","enabled":true,"private_key":"not-allowed"}})
    )
    .is_err());
}
#[test]
fn endpoint_preferences_delete_only_owned_overrides_and_restore_rollback_snapshot() {
    let mut previous =
        candidate(&AppConfig::default(), "a", &source(), &row(), &state(false)).unwrap();
    previous
        .profile_edits
        .entry("b".into())
        .or_default()
        .insert("dns".into(), json!({}));
    let mut next = previous.clone();
    prune(&mut next, &HashSet::from(["b"]));
    assert!(!next.profile_edits.contains_key("a"));
    assert_eq!(next.profile_edits["b"], previous.profile_edits["b"]);
    // Candidate pruning cannot mutate the settings used for rollback or a
    // failed store publication. A restored profile reuses this exact snapshot.
    assert!(has_override(&previous, "a", &row()));
    restore_profile(&mut next, "a", previous.profile_edits.get("a"));
    prune(&mut next, &HashSet::from(["a", "b"]));
    assert_eq!(next.profile_edits, previous.profile_edits);
}

#[test]
fn global_user_endpoint_controls_follow_profile_switches_and_ignore_old_profile_overrides() {
    use crate::configuration::{local_edits, user_endpoints};
    use base64::{engine::general_purpose::STANDARD, Engine};
    let mut app = AppConfig::default();
    app.user_endpoints
        .push(serde_json::json!({"tag":"global-wg","enabled":true,
        "directions":{"inbound":false,"outbound":true},"protocol":{"type":"wireguard",
        "private_key":STANDARD.encode([1;32]),"addresses":["10.2.0.1/32"],
        "peers":[{"public_key":STANDARD.encode([2;32]),"allowed_ips":["0.0.0.0/0"]}]}}));
    app.profile_edits.insert(
        "a".into(),
        std::collections::BTreeMap::from([(
            "endpoints".into(),
            serde_json::json!({"global-wg":{"protocol":"wireguard","enabled":false}}),
        )]),
    );
    let (_, source) = local_edits::resolve(&app, Some("a"), &serde_json::json!({})).unwrap();
    assert_eq!(source["endpoints"][0]["enabled"], true);
    let row = serde_json::json!({"tag":"global-wg","protocol":"wireguard"});
    assert!(owns(&source, &row));
    let next = candidate(
        &app,
        "a",
        &source,
        &row,
        &Action::SetState {
            enabled: false,
            persistence: crate::services::network_endpoint::Persistence::RuntimeOnly,
        },
    )
    .unwrap();
    for id in ["a", "b"] {
        assert_eq!(
            local_edits::resolve(&next, Some(id), &serde_json::json!({}))
                .unwrap()
                .1["endpoints"][0]["enabled"],
            false
        );
    }
    let mut persisted =
        serde_json::from_value::<AppConfig>(serde_json::to_value(next).unwrap()).unwrap();
    prune(&mut persisted, &std::collections::HashSet::new());
    assert_eq!(persisted.user_endpoints.len(), 1);
    let mut source = serde_json::json!({});
    user_endpoints::merge(&mut source, &persisted.user_endpoints).unwrap();
    assert_eq!(source["endpoints"][0]["enabled"], false);
}
