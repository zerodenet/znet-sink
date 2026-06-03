use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::Path;

use super::data_dir;
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
    Ok(DomainStoreData {
        proxy_configs: load_vec(&dir.join(PROXY_CONFIGS_FILE))?,
        subscriptions: load_vec(&dir.join(SUBSCRIPTIONS_FILE))?,
        rule_sets: load_vec(&dir.join(RULE_SETS_FILE))?,
    })
}

pub(crate) fn save_proxy_configs(items: &[ProxyConfigProfile]) -> AppResult<()> {
    save_proxy_configs_to_dir(&data_dir()?, items)
}

pub(crate) fn save_subscriptions(items: &[SubscriptionProfile]) -> AppResult<()> {
    save_subscriptions_to_dir(&data_dir()?, items)
}

pub(crate) fn save_rule_sets(items: &[RuleSetProfile]) -> AppResult<()> {
    save_rule_sets_to_dir(&data_dir()?, items)
}

pub fn save_proxy_configs_to_dir(dir: &Path, items: &[ProxyConfigProfile]) -> AppResult<()> {
    save_vec(&dir.join(PROXY_CONFIGS_FILE), items)
}

pub fn save_subscriptions_to_dir(dir: &Path, items: &[SubscriptionProfile]) -> AppResult<()> {
    save_vec(&dir.join(SUBSCRIPTIONS_FILE), items)
}

pub fn save_rule_sets_to_dir(dir: &Path, items: &[RuleSetProfile]) -> AppResult<()> {
    save_vec(&dir.join(RULE_SETS_FILE), items)
}

fn load_vec<T>(path: &Path) -> AppResult<Vec<T>>
where
    T: DeserializeOwned,
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
    T: Serialize,
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

