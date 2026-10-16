//! Profile-owned route rule metadata. Projection is pure and never applies a
//! runtime configuration or reads rule/GeoIP dependencies.
use crate::errors::{AppError, AppResult};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_ROWS: usize = 1000;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Overlay {
    version: u8,
    source_digest: String,
    rows: Vec<Row>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    enabled: bool,
    rule: Value,
}
fn invalid() -> AppError {
    AppError::invalid_argument("路由规则覆盖的版本、身份、类型或预算不合法；原材料保留")
}
fn parse(metadata: &Value) -> AppResult<Overlay> {
    if serde_json::to_vec(metadata).map_err(|_| invalid())?.len() > MAX_BYTES {
        return Err(invalid());
    }
    let overlay: Overlay = serde_json::from_value(metadata.clone()).map_err(|_| invalid())?;
    if overlay.version != 1
        || overlay.source_digest.len() != 64
        || !overlay
            .source_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        || overlay.rows.len() > MAX_ROWS
    {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    for row in &overlay.rows {
        if row.id.is_empty()
            || row.id.len() > 128
            || !row.id.as_bytes()[0].is_ascii_alphanumeric()
            || !row
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
            || !ids.insert(row.id.clone())
            || !row.rule.is_object()
        {
            return Err(invalid());
        }
    }
    Ok(overlay)
}

/// Validate only the profile-owned metadata envelope. Schema/reference checks
/// need the original source and belong to apply(), including disabled rows.
pub(crate) fn validate(metadata: &Value) -> AppResult<()> {
    parse(metadata).map(|_| ())
}
pub(crate) fn source_digest(base: &Value) -> AppResult<String> {
    super::workspace::digest(base)
}
fn replace_rules(config: &mut Value, rules: Vec<Value>) -> AppResult<()> {
    let root = config.as_object_mut().ok_or_else(invalid)?;
    let route = root
        .entry("route")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(invalid)?;
    route.insert("rules".into(), Value::Array(rules));
    Ok(())
}
pub(crate) fn apply(base: &Value, metadata: &Value) -> AppResult<Value> {
    let overlay = parse(metadata)?;
    if overlay.source_digest != source_digest(base)? {
        return Err(AppError::conflict(
            "route_rules",
            "",
            "规则来源已更新；覆盖未重新绑定，请明确刷新并重建规则草稿",
        ));
    }
    // Check every rule, including disabled rows, against real source targets.
    // native_validation inspects the typed schema and blocks rule_sets/GeoIP
    // before the native parser can reach any filesystem-dependent compilation.
    let mut all = base.clone();
    replace_rules(
        &mut all,
        overlay.rows.iter().map(|row| row.rule.clone()).collect(),
    )?;
    super::native_validation::validate_text(&all.to_string())?;
    replace_rules(
        &mut all,
        overlay
            .rows
            .into_iter()
            .filter(|row| row.enabled)
            .map(|row| row.rule)
            .collect(),
    )?;
    // Keep the original Value representation, not normalized native defaults.
    Ok(all)
}

#[cfg(test)]
#[path = "route_rules_tests.rs"]
mod tests;
