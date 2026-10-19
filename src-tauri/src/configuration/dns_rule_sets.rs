//! DNS may consume client rule assets without enabling their route actions.
use crate::errors::{AppError, AppResult};
use crate::models::rule_set::{RuleSetProfile, ZrsArtifact};
use serde_json::{json, Value};
use std::collections::BTreeSet;

use super::rules::common_tag;

pub(crate) fn available(profile: &RuleSetProfile) -> bool {
    profile.enabled
        && profile.managed_by_subscription_id.is_none()
        && !crate::services::rule_set::is_managed_subscription_rule_set_id(&profile.id)
        && profile.artifact.is_some()
}

pub(crate) fn materialize(
    config: &mut Value,
    profiles: &[RuleSetProfile],
    verify: impl Fn(&RuleSetProfile, &ZrsArtifact) -> AppResult<()>,
) -> AppResult<()> {
    let mut referenced = BTreeSet::new();
    if let Some(dispatch) = config.pointer("/runtime/dns/dispatch") {
        references(dispatch, &mut referenced);
    }
    let definitions = config.pointer("/route/rule_sets").and_then(Value::as_array);
    let mut additions = Vec::new();
    for tag in referenced {
        let profile = profiles
            .iter()
            .find(|profile| common_tag(&profile.id) == tag);
        // Subscription-owned tags retain their configuration-defined identity.
        if profile.is_none() && !tag.starts_with("gui-common-") {
            continue;
        }
        let profile = profile
            .filter(|profile| available(profile))
            .ok_or_else(|| {
                AppError::invalid_argument(format!(
                    "DNS 引用的客户端规则集 {tag} 已删除、禁用或尚未生成 ZRS，请先修改 DNS 分流"
                ))
            })?;
        let artifact = profile.artifact.as_ref().expect("available asset");
        verify(profile, artifact)?;
        if let Some(existing) = definitions
            .into_iter()
            .flatten()
            .find(|item| item["tag"] == tag)
        {
            if existing["type"] != "file"
                || existing["path"] != artifact.path
                || existing["format"] != "zrs"
            {
                return Err(AppError::invalid_argument(format!(
                    "DNS 规则集 {tag} 与现有规则资源冲突"
                )));
            }
            continue;
        }
        additions.push(json!({"tag":tag,"type":"file","path":artifact.path,"format":"zrs"}));
    }
    if additions.is_empty() {
        return Ok(());
    }
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("config must be an object"))?;
    let route = root
        .entry("route")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("route must be an object"))?;
    let definitions = route
        .entry("rule_sets")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| AppError::invalid_argument("route.rule_sets must be an array"))?;
    definitions.extend(additions);
    Ok(())
}

fn references(value: &Value, tags: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("rule_set") {
                if let Some(tag) = object.get("tag").and_then(Value::as_str) {
                    tags.insert(tag.to_owned());
                }
            }
            for child in object.values() {
                references(child, tags);
            }
        }
        Value::Array(values) => {
            for child in values {
                references(child, tags);
            }
        }
        _ => {}
    }
}
