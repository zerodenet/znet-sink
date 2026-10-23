use std::collections::BTreeMap;

use serde_json::Value;

use crate::errors::{AppError, AppResult};
use crate::services::{common, domain_store};
use crate::state::app_state::AppState;

/// Called under the configuration operation lock after Zero accepts a choice.
/// Persist every profile's choices and mirror associated subscriptions atomically.
pub fn record_active_configuration_selection(
    state: &AppState,
    policy_tag: &str,
    target_tag: &str,
) -> AppResult<bool> {
    let mut profiles = common::lock(state.proxy_configs(), "proxy_config")?;
    let mut sources = common::lock(state.subscriptions(), "subscription")?;
    record_selection(
        &mut profiles,
        &mut sources,
        policy_tag,
        target_tag,
        |profiles, sources| domain_store::save_relational_data(profiles, sources),
    )
}

fn record_selection(
    profiles: &mut Vec<crate::models::proxy_config::ProxyConfigProfile>,
    sources: &mut Vec<crate::models::subscription::SubscriptionProfile>,
    policy_tag: &str,
    target_tag: &str,
    persist: impl FnOnce(
        &[crate::models::proxy_config::ProxyConfigProfile],
        &[crate::models::subscription::SubscriptionProfile],
    ) -> AppResult<()>,
) -> AppResult<bool> {
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
    let mut next_sources = sources.clone();
    for source in next_sources
        .iter_mut()
        .filter(|source| source.target_proxy_config_id.as_deref() == Some(active.id.as_str()))
    {
        source
            .policy_selections
            .insert(policy_tag.to_string(), target_tag.to_string());
        source.updated_at_unix_ms = common::now_unix_ms();
    }
    persist(&next_profiles, &next_sources)?;
    *profiles = next_profiles;
    *sources = next_sources;
    Ok(true)
}

/// Legacy subscription choices remain readable; profile-owned choices win.
pub(crate) fn selections_for_profile(
    state: &AppState,
    id: &str,
) -> AppResult<BTreeMap<String, String>> {
    let profiles = common::lock(state.proxy_configs(), "proxy_config")?;
    let sources = common::lock(state.subscriptions(), "subscription")?;
    Ok(resolve_selections(&profiles, &sources, id))
}

fn resolve_selections(
    profiles: &[crate::models::proxy_config::ProxyConfigProfile],
    sources: &[crate::models::subscription::SubscriptionProfile],
    id: &str,
) -> BTreeMap<String, String> {
    let mut selections = BTreeMap::new();
    // Preserve the legacy first-source precedence when several sources share a profile.
    for source in sources
        .iter()
        .filter(|source| source.target_proxy_config_id.as_deref() == Some(id))
    {
        for (group, target) in &source.policy_selections {
            selections
                .entry(group.clone())
                .or_insert_with(|| target.clone());
        }
    }
    if let Some(profile) = profiles.iter().find(|profile| profile.id == id) {
        selections.extend(profile.policy_selections.clone());
    }
    selections
}

pub(crate) fn saved_selections(
    state: &AppState,
    base: &Value,
) -> AppResult<(Option<String>, BTreeMap<String, String>)> {
    let profiles = common::lock(state.proxy_configs(), "proxy_config")?;
    let profile_id = profiles
        .iter()
        .find(|profile| profile.active && profile.content.as_ref() == Some(base))
        .or_else(|| {
            profiles
                .iter()
                .find(|profile| profile.content.as_ref() == Some(base))
        })
        .map(|profile| profile.id.clone());
    drop(profiles);
    let Some(id) = profile_id else {
        return Ok((None, BTreeMap::new()));
    };
    Ok((Some(id.clone()), selections_for_profile(state, &id)?))
}

/// Fallback browsing must show the saved choice, not the source's old default.
pub(crate) fn active_content(state: &AppState) -> AppResult<Option<Value>> {
    let profiles = common::lock(state.proxy_configs(), "proxy_config")?;
    let sources = common::lock(state.subscriptions(), "subscription")?;
    let Some(profile) = profiles.iter().find(|profile| profile.active) else {
        return Ok(None);
    };
    let Some(mut content) = profile.content.clone() else {
        return Ok(None);
    };
    apply_selections(
        &mut content,
        &resolve_selections(&profiles, &sources, &profile.id),
    );
    Ok(Some(content))
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
#[path = "policy_selection_tests.rs"]
mod persistence_tests;

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
