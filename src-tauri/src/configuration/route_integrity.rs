use crate::errors::{AppError, AppResult};
use serde_json::Value;
use std::collections::BTreeSet;

/// Missing policy dependencies are errors; deleting their rules changes the
/// user's routing policy (including reject rules) into a different policy.
pub(crate) fn validate(config: &Value) -> AppResult<()> {
    let Some(route) = config.get("route") else {
        return Ok(());
    };
    let defined: BTreeSet<_> = route
        .get("rule_sets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("tag").and_then(Value::as_str))
        .collect();
    let mut missing = BTreeSet::new();
    for rule in route
        .get("rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(condition) = rule.get("condition") {
            inspect(condition, &defined, &mut missing);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(AppError {
        code: "rule_set_dependency_unavailable",
        message: format!(
            "规则集不可用，已拒绝应用不完整的路由配置：{}",
            missing.iter().copied().collect::<Vec<_>>().join(", ")
        ),
        details: Some(serde_json::json!({"missingRuleSets": missing})),
    })
}

fn inspect<'a>(value: &'a Value, defined: &BTreeSet<&str>, missing: &mut BTreeSet<&'a str>) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("rule_set") {
                if let Some(tag) = object.get("tag").and_then(Value::as_str) {
                    if !defined.contains(tag) {
                        missing.insert(tag);
                    }
                }
            }
            for child in object.values() {
                inspect(child, defined, missing);
            }
        }
        Value::Array(values) => {
            for child in values {
                inspect(child, defined, missing);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "route_integrity_tests.rs"]
mod tests;
