use crate::errors::{AppError, AppResult};
use crate::models::{
    gui_core::GuiProxyMode,
    rule_set::{CommonRuleAction, RuleSetProfile},
};
use crate::services::{proxy_mode, rule_set};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
const COMMON_TAG_PREFIX: &str = "gui-common-";

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ComposeResult {
    pub config: Value,
    pub injected_count: usize,
}

pub(crate) fn compose_with(
    base: &Value,
    enabled: bool,
    profiles: &[RuleSetProfile],
    verify_artifact: impl Fn(&RuleSetProfile, &crate::models::rule_set::ZrsArtifact) -> AppResult<()>,
) -> AppResult<ComposeResult> {
    let mut config = base.clone();
    strip_common_overlay(&mut config)?;
    crate::configuration::route_integrity::validate(&config)?;
    let mode = proxy_mode::detect_route_mode(base).map(|detected| match detected.mode {
        GuiProxyMode::Global => "global".to_string(),
        GuiProxyMode::Rule => "rule".to_string(),
        GuiProxyMode::Direct => "direct".to_string(),
    });
    if !enabled {
        return Ok(ComposeResult {
            config,
            injected_count: 0,
        });
    }
    if mode.as_deref() != Some("rule") {
        return Ok(ComposeResult {
            config,
            injected_count: 0,
        });
    }

    let mut selected = eligible_profiles(profiles).collect::<Vec<_>>();
    selected.sort_by_key(|profile| {
        let binding = profile
            .common_binding
            .as_ref()
            .expect("eligible profile has binding");
        (binding.order, profile.id.as_str())
    });
    if selected.is_empty() {
        return Ok(ComposeResult {
            config,
            injected_count: 0,
        });
    }

    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("proxy config must be a JSON object"))?;
    let route = object_field(root, "route")?;
    let final_action = route
        .get("final")
        .cloned()
        .unwrap_or_else(|| json!({ "type": "direct" }));
    let rule_sets = array_field(route, "rule_sets")?;
    let mut definitions = Vec::with_capacity(selected.len());
    let mut rules = Vec::with_capacity(selected.len());
    for profile in selected {
        let artifact = profile
            .artifact
            .as_ref()
            .expect("eligible profile has artifact");
        verify_artifact(profile, artifact)?;
        let tag = common_tag(&profile.id);
        definitions
            .push(json!({ "tag": tag, "type": "file", "path": artifact.path, "format": "zrs" }));
        let action = match &profile
            .common_binding
            .as_ref()
            .expect("eligible profile has binding")
            .action
        {
            CommonRuleAction::Final => final_action.clone(),
            CommonRuleAction::Proxy => json!({
                "type": "route",
                "outbound": proxy_mode::resolve_global_outbound(base, None)
            }),
            CommonRuleAction::Direct => json!({ "type": "direct" }),
            CommonRuleAction::Reject => json!({ "type": "reject" }),
        };
        rules.push(json!({ "condition": { "type": "rule_set", "tag": tag }, "action": action }));
    }
    let injected_count = rules.len();
    rule_sets.extend(definitions);
    let existing_rules = array_field(route, "rules")?;
    // Subscription rules are usually more specific than GUI-wide rules
    // (for example, an AI service group versus the broad built-in GFW
    // domain set). Preserve those semantics by evaluating the subscription
    // first and using common rules only as a fallback before `route.final`.
    existing_rules.extend(rules);
    Ok(ComposeResult {
        config,
        injected_count,
    })
}

pub(crate) fn eligible_profiles(
    profiles: &[RuleSetProfile],
) -> impl Iterator<Item = &RuleSetProfile> {
    profiles.iter().filter(|profile| {
        profile.enabled
            && profile.managed_by_subscription_id.is_none()
            && !rule_set::is_managed_subscription_rule_set_id(&profile.id)
            && profile.artifact.is_some()
            && profile
                .common_binding
                .as_ref()
                .is_some_and(|binding| binding.enabled)
    })
}

pub(crate) fn common_tag(id: &str) -> String {
    let digest = Sha256::digest(id.as_bytes());
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{COMMON_TAG_PREFIX}{suffix}")
}

fn strip_common_overlay(config: &mut Value) -> AppResult<()> {
    let Some(root) = config.as_object_mut() else {
        return Err(AppError::invalid_argument(
            "proxy config must be a JSON object",
        ));
    };
    let Some(route) = root.get_mut("route").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    if let Some(items) = route.get_mut("rule_sets").and_then(Value::as_array_mut) {
        items.retain(|item| {
            !item
                .get("tag")
                .and_then(Value::as_str)
                .is_some_and(|tag| tag.starts_with(COMMON_TAG_PREFIX))
        });
    }
    if let Some(items) = route.get_mut("rules").and_then(Value::as_array_mut) {
        items.retain(|item| {
            !item
                .get("condition")
                .and_then(|condition| condition.get("tag"))
                .and_then(Value::as_str)
                .is_some_and(|tag| tag.starts_with(COMMON_TAG_PREFIX))
        });
    }
    Ok(())
}

fn object_field<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> AppResult<&'a mut Map<String, Value>> {
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), Value::Object(Map::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::invalid_argument(format!("{key} must be an object")))
}

fn array_field<'a>(root: &'a mut Map<String, Value>, key: &str) -> AppResult<&'a mut Vec<Value>> {
    if !root.contains_key(key) {
        root.insert(key.to_string(), Value::Array(Vec::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::invalid_argument(format!("route.{key} must be an array")))
}
