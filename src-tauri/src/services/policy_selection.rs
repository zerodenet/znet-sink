use std::collections::BTreeMap;

use serde_json::Value;

use crate::errors::{AppError, AppResult};
use crate::services::{common, domain_store};
use crate::state::app_state::AppState;

/// Additive, idempotent upgrade: configuration choices win, then legacy sources
/// in their original order. Keep every source and its original choices intact.
pub(crate) fn inherit_legacy_selections(
    profiles: &mut [crate::models::proxy_config::ProxyConfigProfile],
    subscriptions: &[crate::models::subscription::SubscriptionProfile],
) -> bool {
    let mut changed = false;
    for profile in profiles {
        for source in subscriptions
            .iter()
            .filter(|source| source.target_proxy_config_id.as_deref() == Some(profile.id.as_str()))
        {
            for (group, target) in &source.policy_selections {
                if !profile.policy_selections.contains_key(group) {
                    profile
                        .policy_selections
                        .insert(group.clone(), target.clone());
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Called under the configuration operation lock after Zero accepts a choice.
/// Persist against the configuration and mirror existing sources for old SDKs.
pub fn record_active_configuration_selection(
    state: &AppState,
    policy_tag: &str,
    target_tag: &str,
) -> AppResult<bool> {
    let mut profiles = common::lock(state.proxy_configs(), "proxy_config")?;
    let mut next_profiles = profiles.clone();
    let Some(active) = next_profiles.iter_mut().find(|profile| profile.active) else {
        return Ok(false);
    };
    let Some(content) = active.content.as_ref() else {
        return Ok(false);
    };
    if !selection_is_valid(content, policy_tag, target_tag) {
        return Err(AppError::invalid_argument(format!(
            "policy target '{target_tag}' is not a member of selector '{policy_tag}'"
        )));
    }
    active
        .policy_selections
        .insert(policy_tag.to_string(), target_tag.to_string());
    let mut subscriptions = common::lock(state.subscriptions(), "subscription")?;
    let mut next_sources = subscriptions.clone();
    for source in next_sources
        .iter_mut()
        .filter(|source| source.target_proxy_config_id.as_deref() == Some(active.id.as_str()))
    {
        source
            .policy_selections
            .insert(policy_tag.to_string(), target_tag.to_string());
    }
    domain_store::save_relational_data(&next_profiles, &next_sources)?;
    *profiles = next_profiles;
    *subscriptions = next_sources;
    Ok(true)
}

/// Apply saved choices to a config before it is exported or sent to Zero.
/// Invalid/stale choices are ignored so Zero uses its normal selection policy.
pub(crate) fn saved_selections(
    state: &AppState,
    base: &Value,
) -> AppResult<(Option<String>, BTreeMap<String, String>)> {
    let profiles = common::lock(state.proxy_configs(), "proxy_config")?;
    let profile = profiles
        .iter()
        .find(|profile| profile.active && profile.content.as_ref() == Some(base))
        .or_else(|| {
            profiles
                .iter()
                .find(|profile| profile.content.as_ref() == Some(base))
        });
    Ok(profile
        .map(|profile| (Some(profile.id.clone()), profile.policy_selections.clone()))
        .unwrap_or_else(|| (None, BTreeMap::new())))
}

/// Explicit configuration identity prevents equal JSON sources sharing choices.
pub(crate) fn selections_for_profile(
    state: &AppState,
    id: &str,
) -> AppResult<BTreeMap<String, String>> {
    Ok(common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.id == id)
        .map(|profile| profile.policy_selections.clone())
        .unwrap_or_default())
}

pub fn retain_valid_selections(selections: &mut BTreeMap<String, String>, config: &Value) {
    selections.retain(|policy, target| selection_is_valid(config, policy, target));
}

pub fn apply_selections(config: &mut Value, selections: &BTreeMap<String, String>) {
    let Some(groups) = config
        .get_mut("outbound_groups")
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for group in groups {
        let Some(object) = group.as_object_mut() else {
            continue;
        };
        let Some(tag) = object.get("tag").and_then(Value::as_str) else {
            continue;
        };
        let Some(target) = selections.get(tag) else {
            continue;
        };
        let valid = object
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "selector")
            && object
                .get("outbounds")
                .and_then(Value::as_array)
                .is_some_and(|members| {
                    members.iter().any(|member| member.as_str() == Some(target))
                });
        if valid {
            object.insert("selected".to_string(), Value::String(target.clone()));
        }
    }
}

fn selection_is_valid(config: &Value, policy_tag: &str, target_tag: &str) -> bool {
    config
        .get("outbound_groups")
        .and_then(Value::as_array)
        .and_then(|groups| {
            groups.iter().find(|group| {
                group.get("tag").and_then(Value::as_str) == Some(policy_tag)
                    && group.get("type").and_then(Value::as_str) == Some("selector")
            })
        })
        .and_then(|group| group.get("outbounds"))
        .and_then(Value::as_array)
        .is_some_and(|members| {
            members
                .iter()
                .any(|member| member.as_str() == Some(target_tag))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_selector_is_applied_only_while_group_and_member_exist() {
        let mut selections = BTreeMap::from([("Proxy".to_string(), "node-b".to_string())]);
        let mut config = json!({
            "outbound_groups": [{
                "tag": "Proxy",
                "type": "selector",
                "outbounds": ["node-a", "node-b"]
            }]
        });

        apply_selections(&mut config, &selections);
        assert_eq!(config["outbound_groups"][0]["selected"], "node-b");

        config["outbound_groups"][0]["outbounds"] = json!(["node-a"]);
        config["outbound_groups"][0]
            .as_object_mut()
            .unwrap()
            .remove("selected");
        retain_valid_selections(&mut selections, &config);
        assert!(selections.is_empty());
        apply_selections(&mut config, &selections);
        assert!(config["outbound_groups"][0].get("selected").is_none());
    }

    #[test]
    fn automatic_groups_never_receive_persisted_manual_selection() {
        let selections = BTreeMap::from([("Auto".to_string(), "node-b".to_string())]);
        let mut config = json!({
            "outbound_groups": [{
                "tag": "Auto",
                "type": "url_test",
                "outbounds": ["node-a", "node-b"]
            }]
        });
        apply_selections(&mut config, &selections);
        assert!(config["outbound_groups"][0].get("selected").is_none());
    }
}
