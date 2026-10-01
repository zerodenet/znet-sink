//! Client-owned sparse endpoint preferences, scoped to a source profile.
use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppConfig;
use crate::services::network_endpoint::{Action, Directions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directions: Option<Directions>,
}
type Preferences = BTreeMap<String, Preference>;
fn parse(value: &Value) -> AppResult<Preferences> {
    serde_json::from_value(value.clone())
        .map_err(|error| AppError::invalid_argument(format!("端点本地覆盖无效：{error}")))
}
pub(crate) fn validate(value: &Value) -> AppResult<()> {
    parse(value).map(|_| ())
}
pub(crate) fn owns(source: &Value, endpoint: &Value) -> bool {
    source
        .get("endpoints")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .filter(|row| {
                    row["tag"] == endpoint["tag"]
                        && row.pointer("/protocol/type") == endpoint.get("protocol")
                        && row["tag"].as_str().is_some_and(|tag| !tag.is_empty())
                })
                .count()
                == 1
        })
}
pub(crate) fn has_override(app: &AppConfig, profile: &str, endpoint: &Value) -> bool {
    app.profile_edits
        .get(profile)
        .and_then(|edits| edits.get("endpoints"))
        .and_then(|value| value.get(endpoint["tag"].as_str()?))
        .is_some_and(|value| value["protocol"] == endpoint["protocol"])
}
pub(crate) fn candidate(
    app: &AppConfig,
    profile: &str,
    source: &Value,
    endpoint: &Value,
    action: &Action,
) -> AppResult<AppConfig> {
    if !owns(source, endpoint) {
        return Err(AppError::conflict(
            "endpoint",
            "",
            "端点不属于当前配置的端点声明，请刷新",
        ));
    }
    let mut next = app.clone();
    if matches!(action, Action::Restart {}) {
        return Ok(next);
    }
    let edits = next.profile_edits.entry(profile.into()).or_default();
    let mut prefs = edits
        .get("endpoints")
        .map(parse)
        .transpose()?
        .unwrap_or_default();
    let tag = endpoint["tag"].as_str().unwrap();
    let protocol = endpoint["protocol"].as_str().unwrap();
    let preference = prefs.entry(tag.into()).or_default();
    if preference.protocol != protocol {
        *preference = Preference {
            protocol: protocol.into(),
            ..Default::default()
        };
    }
    match action {
        Action::SetState { enabled, .. } => preference.enabled = Some(*enabled),
        Action::SetDirections { directions, .. } => {
            preference.directions = Some(directions.clone())
        }
        _ => return Err(AppError::invalid_argument("不支持的端点本地覆盖操作")),
    }
    edits.insert(
        "endpoints".into(),
        serde_json::to_value(prefs).map_err(|e| AppError::internal(e.to_string()))?,
    );
    Ok(next)
}
pub(crate) fn apply(source: &mut Value, preferences: &Value) -> AppResult<()> {
    let preferences = parse(preferences)?;
    if let Some(rows) = source.get_mut("endpoints").and_then(Value::as_array_mut) {
        for row in rows {
            let Some(preference) = row["tag"].as_str().and_then(|tag| preferences.get(tag)) else {
                continue;
            };
            // A removed endpoint or a reused tag must never create a role or
            // carry an old protocol's intent into a different resource.
            if row.pointer("/protocol/type").and_then(Value::as_str) != Some(&preference.protocol) {
                continue;
            }
            if let Some(enabled) = preference.enabled {
                row["enabled"] = Value::Bool(enabled);
            }
            if let Some(directions) = &preference.directions {
                row["directions"] = serde_json::to_value(directions).unwrap();
            }
        }
    }
    Ok(())
}
pub(crate) fn store(state: &crate::state::app_state::AppState, app: AppConfig) -> AppResult<()> {
    let mut current = crate::services::common::lock(state.app_config(), "app_config")?;
    crate::services::app_config_store::save(
        &crate::services::app_config_store::default_config_path()?,
        &app,
    )?;
    *current = app;
    Ok(())
}
pub(crate) fn restore_profile(
    app: &mut AppConfig,
    id: &str,
    edits: Option<&super::local_edits::Edits>,
) {
    if let Some(edits) = edits {
        app.profile_edits.insert(id.into(), edits.clone());
    } else {
        app.profile_edits.remove(id);
    }
}
pub(crate) fn prune(app: &mut AppConfig, retained: &HashSet<&str>) {
    app.profile_edits.retain(|id, edits| {
        if !retained.contains(id.as_str()) {
            edits.remove("endpoints");
        }
        !edits.is_empty()
    });
}

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
