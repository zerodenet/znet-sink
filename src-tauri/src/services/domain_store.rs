use serde::Deserialize;
use std::fs;
use std::path::Path;

use super::{app_database, data_dir};
use crate::errors::{AppError, AppResult};
use crate::models::{
    proxy_config::ProxyConfigProfile, rule_set::RuleSetProfile, subscription::SubscriptionProfile,
};

const PROXY_CONFIGS_FILE: &str = "proxy-configs.json";
const SUBSCRIPTIONS_FILE: &str = "subscriptions.json";
const RULE_SETS_FILE: &str = "rule-sets.json";

#[derive(Default)]
pub struct DomainStoreData {
    pub proxy_configs: Vec<ProxyConfigProfile>,
    pub subscriptions: Vec<SubscriptionProfile>,
    pub rule_sets: Vec<RuleSetProfile>,
}

pub(crate) fn load_all() -> AppResult<DomainStoreData> {
    load_all_from_dir(&data_dir()?)
}

pub fn load_all_from_dir(dir: &Path) -> AppResult<DomainStoreData> {
    let relational = app_database::load_domain_data(dir)?;
    Ok(DomainStoreData {
        proxy_configs: relational.proxy_configs,
        subscriptions: relational.subscriptions,
        rule_sets: load_rule_sets(&dir.join(RULE_SETS_FILE))?,
    })
}

fn load_rule_sets(path: &Path) -> AppResult<Vec<RuleSetProfile>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read data store: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    let mut values: Vec<serde_json::Value> =
        serde_json::from_str(&content).map_err(|error| AppError {
            code: "invalid_argument",
            message: format!("failed to parse data store: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
    for value in &mut values {
        let Some(object) = value.as_object_mut() else {
            continue;
        };
        if !object.contains_key("semanticIr") {
            if let Some(ir) = object.remove("compiledIr") {
                object.insert("semanticIr".into(), ir);
            }
        }
        if object
            .get("semanticIr")
            .is_none_or(serde_json::Value::is_null)
        {
            let name = object
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("Migrated");
            object.insert(
                "semanticIr".into(),
                serde_json::json!({ "version": 1, "name": name, "rules": [] }),
            );
        }
        let legacy_format = match object
            .get("format")
            .and_then(|value| value.as_str())
            .unwrap_or("auto")
        {
            "zero" | "zero-rule" | "zero-rule-ir" | "zero-rule-ir-v1" => "zero-rule-ir-v1",
            "clash" | "clash-yaml" | "clash-classical" | "clash-classical-yaml" => {
                "clash-classical-yaml"
            }
            _ => "auto",
        }
        .to_string();
        if let Some(source) = object.get_mut("source") {
            let remote_url = source
                .get("url")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            match remote_url {
                Some(url) => *source = serde_json::json!({ "url": url, "format": legacy_format }),
                None => {
                    object.remove("source");
                }
            }
        }
        object.remove("format");
        object.remove("ruleCount");
        object.remove("lastUpdatedAtUnixMs");
    }
    serde_json::from_value(serde_json::Value::Array(values)).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to migrate rule set data: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

pub(crate) fn save_subscriptions(items: &[SubscriptionProfile]) -> AppResult<()> {
    save_subscriptions_to_dir(&data_dir()?, items)
}

pub(crate) fn save_rule_sets(items: &[RuleSetProfile]) -> AppResult<()> {
    save_rule_sets_to_dir(&data_dir()?, items)
}

pub fn save_proxy_configs_to_dir(dir: &Path, items: &[ProxyConfigProfile]) -> AppResult<()> {
    app_database::save_proxy_configs(dir, items)
}

pub fn save_subscriptions_to_dir(dir: &Path, items: &[SubscriptionProfile]) -> AppResult<()> {
    app_database::save_subscriptions(dir, items)
}

pub fn save_rule_sets_to_dir(dir: &Path, items: &[RuleSetProfile]) -> AppResult<()> {
    save_vec(&dir.join(RULE_SETS_FILE), items)
}

pub(crate) fn save_relational_data(
    proxy_configs: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
) -> AppResult<()> {
    app_database::save_domain_data(&data_dir()?, proxy_configs, subscriptions)
}

pub(crate) fn legacy_proxy_configs_path(dir: &Path) -> std::path::PathBuf {
    dir.join(PROXY_CONFIGS_FILE)
}

pub(crate) fn legacy_subscriptions_path(dir: &Path) -> std::path::PathBuf {
    dir.join(SUBSCRIPTIONS_FILE)
}

pub(crate) fn load_legacy_vec<T>(path: &Path) -> AppResult<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read data store: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;

    serde_json::from_str(&content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to parse data store: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn save_vec<T>(path: &Path, items: &[T]) -> AppResult<()>
where
    T: serde::Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to create data store directory: {error}"),
            details: Some(serde_json::json!({ "path": parent.display().to_string() })),
        })?;
    }

    let content = serde_json::to_string_pretty(items).map_err(|error| AppError {
        code: "internal",
        message: format!("failed to serialize data store: {error}"),
        details: None,
    })?;

    fs::write(path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write data store: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}
