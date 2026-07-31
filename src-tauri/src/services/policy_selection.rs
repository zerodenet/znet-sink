use std::collections::BTreeMap;

use serde_json::Value;

use crate::errors::{AppError, AppResult};
use crate::services::{common, domain_store};
use crate::state::app_state::AppState;

/// Persist a successful selector change against the subscription that owns
/// the active proxy profile. Non-subscription profiles intentionally remain
/// runtime-only.
pub fn record_active_subscription_selection(
    state: &AppState,
    policy_tag: &str,
    target_tag: &str,
) -> AppResult<bool> {
    let active = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .cloned();
    let Some(active) = active else {
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

    let mut subscriptions = common::lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    let Some(subscription) = next
        .iter_mut()
        .find(|item| item.target_proxy_config_id.as_deref() == Some(active.id.as_str()))
    else {
        return Ok(false);
    };
    subscription
        .policy_selections
        .insert(policy_tag.to_string(), target_tag.to_string());
    subscription.updated_at_unix_ms = common::now_unix_ms();
    domain_store::save_subscriptions(&next)?;
    *subscriptions = next;
    Ok(true)
}

/// Apply saved choices to a config before it is exported or sent to Zero.
/// Invalid/stale choices are ignored so Zero uses the group's first member.
pub fn apply_saved_selections(state: &AppState, base: &Value, config: &mut Value) -> AppResult<()> {
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
    let Some(profile_id) = profile_id else {
        return Ok(());
    };
    let selections = common::lock(state.subscriptions(), "subscription")?
        .iter()
        .find(|item| item.target_proxy_config_id.as_deref() == Some(profile_id.as_str()))
        .map(|item| item.policy_selections.clone())
        .unwrap_or_default();
    apply_selections(config, &selections);
    Ok(())
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
