//! Closed native-condition reference projection. No file, network, compile or
//! runtime operations are reachable from this module.
use crate::{
    errors::{AppError, AppResult},
    models::rule_references::{Reference, Replacement},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

mod prepare;
pub(crate) use prepare::prepare;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_REFERENCES: usize = 4096;
const MAX_REPLACEMENTS: usize = 1000;
const MAX_DEPTH: usize = 64;

fn invalid() -> AppError {
    AppError {
        code: "rule_reference_invalid",
        message: "规则集引用的结构、身份或预算无法安全核对；原材料保留".into(),
        details: None,
    }
}
fn budget(value: &Value) -> AppResult<()> {
    encoded_size(value).map(|_| ())
}
fn encoded_size(value: &Value) -> AppResult<usize> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|v| *v <= MAX_BYTES)
                .ok_or_else(|| std::io::Error::other("budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter(0);
    serde_json::to_writer(&mut count, value).map_err(|_| invalid())?;
    Ok(count.0)
}
fn tag(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn replacements(values: &[Replacement]) -> AppResult<()> {
    let mut seen = BTreeSet::new();
    if values.len() > MAX_REPLACEMENTS {
        return Err(invalid());
    }
    for value in values {
        if !tag(&value.from_tag)
            || !tag(&value.to_tag)
            || value.from_tag == value.to_tag
            || !seen.insert(&value.from_tag)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn walk(
    value: &Value,
    path: &str,
    kind: &'static str,
    depth: usize,
    refs: &mut Vec<Reference>,
) -> AppResult<()> {
    if depth > MAX_DEPTH {
        return Err(invalid());
    }
    let object = value.as_object().ok_or_else(invalid)?;
    match object.get("type").and_then(Value::as_str) {
        Some("rule_set") => {
            let value = object
                .get("tag")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if !tag(value) || refs.len() >= MAX_REFERENCES {
                return Err(invalid());
            }
            refs.push(Reference {
                profile_id: None,
                field_path: format!("{path}.tag"),
                tag: value.into(),
                kind,
            });
        }
        Some("and" | "or") => {
            for (index, child) in object
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(invalid)?
                .iter()
                .enumerate()
            {
                walk(
                    child,
                    &format!("{path}.items[{index}]"),
                    kind,
                    depth + 1,
                    refs,
                )?;
            }
        }
        Some("inbound" | "domain" | "domain_keyword" | "domain_regex" | "ip" | "geoip" | "sni") => {
        }
        _ => return Err(invalid()),
    }
    Ok(())
}
fn condition(
    value: &Value,
    path: &str,
    kind: &'static str,
    refs: &mut Vec<Reference>,
) -> AppResult<()> {
    // Walk first: cap depth before serde's recursive public native type.
    walk(value, path, kind, 0, refs)?;
    serde_json::from_value::<zero_config::RuleConditionConfig>(value.clone())
        .map_err(|_| invalid())?;
    Ok(())
}

fn opaque_header(value: &Value) -> AppResult<()> {
    if value.as_object().is_some_and(|object| {
        object.get("type").and_then(Value::as_str) == Some("rule_set")
            || object.contains_key("rule_set")
            || object.get("tag").is_some_and(Value::is_string)
    }) {
        return Err(invalid());
    }
    Ok(())
}
/// Additive client DNS fields are preserved, but potential reference structures
/// outside the native dispatch condition cannot be certified as unrelated.
/// Never search/replace arbitrary strings or guess a future field's meaning.
fn reject_opaque(value: &Value, depth: usize, visited: &mut usize) -> AppResult<()> {
    *visited = visited.checked_add(1).ok_or_else(invalid)?;
    if depth > MAX_DEPTH || *visited > 100_000 {
        return Err(invalid());
    }
    opaque_header(value)?;
    match value {
        Value::Object(object) => {
            for value in object.values() {
                reject_opaque(value, depth + 1, visited)?;
            }
        }
        Value::Array(array) => {
            for value in array {
                reject_opaque(value, depth + 1, visited)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn scan_dispatch(config: &Value, prefix: &str, kind: &'static str) -> AppResult<Vec<Reference>> {
    let mut refs = Vec::new();
    let object = config.as_object().ok_or_else(invalid)?;
    opaque_header(config)?;
    let mut visited = 0;
    for (key, value) in object {
        if key != "dispatch" {
            reject_opaque(value, 0, &mut visited)?;
        }
    }
    if let Some(rows) = object.get("dispatch") {
        for (i, row) in rows.as_array().ok_or_else(invalid)?.iter().enumerate() {
            let value = row.get("condition").ok_or_else(invalid)?;
            // The entire row has a closed native shape, not a free-form .tag.
            condition(
                value,
                &format!("{prefix}dispatch[{i}].condition"),
                kind,
                &mut refs,
            )?;
            serde_json::from_value::<zero_config::DnsDispatchRuleConfig>(row.clone())
                .map_err(|_| invalid())?;
        }
    }
    Ok(refs)
}
pub(crate) fn scan_dns(config: &Value) -> AppResult<Vec<Reference>> {
    budget(config)?;
    scan_dispatch(config, "", "dns_dispatch")
}
pub(crate) fn scan(config: &Value) -> AppResult<Vec<Reference>> {
    budget(config)?;
    let mut refs = Vec::new();
    if let Some(route) = config.get("route") {
        let route = route.as_object().ok_or_else(invalid)?;
        if let Some(rows) = route.get("rules") {
            for (i, row) in rows.as_array().ok_or_else(invalid)?.iter().enumerate() {
                let value = row.get("condition").ok_or_else(invalid)?;
                condition(
                    value,
                    &format!("route.rules[{i}].condition"),
                    "route_rule",
                    &mut refs,
                )?;
            }
        }
        if let Some(rows) = route.get("bypass") {
            for (i, value) in rows.as_array().ok_or_else(invalid)?.iter().enumerate() {
                condition(value, &format!("route.bypass[{i}]"), "bypass", &mut refs)?;
            }
        }
    }
    if let Some(dns) = config.pointer("/runtime/dns") {
        refs.extend(scan_dispatch(dns, "runtime.dns.", "dns_dispatch")?);
    }
    if refs.len() > MAX_REFERENCES {
        return Err(invalid());
    }
    // Only deserialize. RuntimeConfig::validate/parse would compile dependencies.
    let typed = serde_json::from_value::<zero_config::RuntimeConfig>(config.clone())
        .map_err(|_| invalid())?;
    if typed.schema_version != zero_config::CONFIG_SCHEMA_VERSION {
        return Err(invalid());
    }
    Ok(refs)
}
pub(crate) fn scan_route_rows(metadata: &Value) -> AppResult<Vec<Reference>> {
    budget(metadata)?;
    super::route_rules::validate(metadata)?;
    let mut refs = Vec::new();
    for (i, row) in metadata["rows"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .enumerate()
    {
        let rule = row.get("rule").ok_or_else(invalid)?;
        condition(
            rule.get("condition").ok_or_else(invalid)?,
            &format!("rows[{i}].rule.condition"),
            "route_overlay",
            &mut refs,
        )?;
        serde_json::from_value::<zero_config::RouteRuleConfig>(rule.clone())
            .map_err(|_| invalid())?;
    }
    Ok(refs)
}
fn rewrite_condition(value: &mut Value, values: &[Replacement]) {
    match value.get("type").and_then(Value::as_str) {
        Some("rule_set") => {
            if let Some(next) = values
                .iter()
                .find(|v| value.get("tag").and_then(Value::as_str) == Some(v.from_tag.as_str()))
            {
                value["tag"] = json!(next.to_tag);
            }
        }
        Some("and" | "or") => {
            if let Some(items) = value.get_mut("items").and_then(Value::as_array_mut) {
                for child in items {
                    rewrite_condition(child, values);
                }
            }
        }
        _ => {}
    }
}
fn rewrite_dispatch(config: &mut Value, values: &[Replacement]) {
    if let Some(rows) = config.get_mut("dispatch").and_then(Value::as_array_mut) {
        for row in rows {
            rewrite_condition(&mut row["condition"], values);
        }
    }
}
pub(crate) fn rewrite_dns(config: &Value, values: &[Replacement]) -> AppResult<Value> {
    replacements(values)?;
    scan_dns(config)?;
    let mut next = config.clone();
    rewrite_dispatch(&mut next, values);
    budget(&next)?;
    Ok(next)
}
pub(crate) fn rewrite(config: &Value, values: &[Replacement]) -> AppResult<Value> {
    replacements(values)?;
    scan(config)?;
    let mut next = config.clone();
    if let Some(route) = next.get_mut("route") {
        if let Some(rows) = route.get_mut("rules").and_then(Value::as_array_mut) {
            for row in rows {
                rewrite_condition(&mut row["condition"], values);
            }
        }
        if let Some(rows) = route.get_mut("bypass").and_then(Value::as_array_mut) {
            for row in rows {
                rewrite_condition(row, values);
            }
        }
    }
    if let Some(dns) = next.pointer_mut("/runtime/dns") {
        rewrite_dispatch(dns, values);
    }
    budget(&next)?;
    Ok(next)
}
pub(crate) fn rewrite_route_rows(metadata: &Value, values: &[Replacement]) -> AppResult<Value> {
    replacements(values)?;
    scan_route_rows(metadata)?;
    let mut next = metadata.clone();
    for row in next["rows"].as_array_mut().ok_or_else(invalid)? {
        rewrite_condition(&mut row["rule"]["condition"], values);
    }
    budget(&next)?;
    Ok(next)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    version: u8,
    source_digest: String,
    replacements: Vec<Replacement>,
}
fn parse_metadata(value: &Value) -> AppResult<Metadata> {
    budget(value)?;
    let value: Metadata = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    if value.version != 1
        || value.source_digest.len() != 64
        || !value
            .source_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    {
        return Err(invalid());
    }
    replacements(&value.replacements)?;
    if value
        .replacements
        .iter()
        .any(|r| !common_tag(&r.from_tag) || !common_tag(&r.to_tag))
    {
        return Err(invalid());
    }
    Ok(value)
}
fn common_tag(value: &str) -> bool {
    value.strip_prefix("gui-common-").is_some_and(|s| {
        s.len() == 16
            && s.bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
    })
}
pub(crate) fn validate_metadata(value: &Value) -> AppResult<()> {
    parse_metadata(value).map(|_| ())
}
pub(crate) fn metadata(source: &Value, values: &[Replacement]) -> AppResult<Value> {
    replacements(values)?;
    if values
        .iter()
        .any(|r| !common_tag(&r.from_tag) || !common_tag(&r.to_tag))
    {
        return Err(invalid());
    }
    budget(source)?;
    Ok(
        json!({"version":1,"sourceDigest":super::route_rules::source_digest(source)?,"replacements":values}),
    )
}
pub(crate) fn apply(source: &Value, config: &Value, value: &Value) -> AppResult<Value> {
    let value = parse_metadata(value)?;
    if value.source_digest != super::route_rules::source_digest(source)? {
        return Err(AppError::conflict(
            "rule_references",
            "",
            "规则来源已变化；原引用迁移材料保留，请重新预览",
        ));
    }
    rewrite(config, &value.replacements)
}

#[cfg(test)]
#[path = "rule_references/tests.rs"]
mod tests;
