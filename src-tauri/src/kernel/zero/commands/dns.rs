use super::{normalize_non_empty, normalize_optional, run_command};
use crate::kernel::zero::parsing::parse_fake_ip_clear;
use crate::{
    errors::AppResult,
    models::{core::CoreIpcOptions, gui_core::GuiFakeIpClearResult},
};
use serde_json::{json, Map, Value};

/// DNS lookup diagnostic.
pub async fn dns_lookup(hostname: String, options: Option<CoreIpcOptions>) -> AppResult<Value> {
    let hostname = normalize_non_empty(hostname, "hostname")?;
    let value = run_command(
        "diagnostics.dns_lookup",
        json!({ "hostname": hostname }),
        options,
    )
    .await?;
    diagnostic_command_result(value)
}

pub async fn dns_cache(
    domain: Option<String>,
    limit: Option<usize>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let mut params = Map::new();
    if let Some(domain) = normalize_optional(domain) {
        params.insert("domain".to_string(), json!(domain));
    }
    if let Some(limit) = limit {
        params.insert("limit".to_string(), json!(limit));
    }
    let value = run_command("diagnostics.dns_cache", Value::Object(params), options).await?;
    diagnostic_command_result(value)
}

pub async fn fakeip_lookup(
    domain: Option<String>,
    ip: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let domain = normalize_optional(domain);
    let ip = normalize_optional(ip);
    if domain.is_some() == ip.is_some() {
        return Err(crate::errors::AppError::invalid_argument(
            "exactly one of domain or ip is required for Fake-IP lookup",
        ));
    }
    let mut params = Map::new();
    if let Some(domain) = domain {
        params.insert("domain".to_string(), json!(domain));
    }
    if let Some(ip) = ip {
        params.insert("ip".to_string(), json!(ip));
    }
    let value = run_command("diagnostics.fakeip_lookup", Value::Object(params), options).await?;
    diagnostic_command_result(value)
}

fn diagnostic_command_result(value: Value) -> AppResult<Value> {
    if value.get("accepted").and_then(Value::as_bool) == Some(false) {
        return Err(crate::errors::AppError::core_response(value));
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

/// Clear all Fake-IP mappings or one mapping selected by domain/address.
pub async fn clear_fake_ip(
    domain: Option<String>,
    ip: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiFakeIpClearResult> {
    let params = fake_ip_clear_params(domain, ip)?;
    let value = run_command("fakeip.clear", params, options).await?;
    Ok(parse_fake_ip_clear(&value))
}

pub(super) fn fake_ip_clear_params(domain: Option<String>, ip: Option<String>) -> AppResult<Value> {
    let domain = normalize_optional(domain);
    let ip = normalize_optional(ip);
    if domain.is_some() && ip.is_some() {
        return Err(crate::errors::AppError::invalid_argument(
            "fake-IP clear accepts at most one of domain or ip",
        ));
    }

    let mut params = Map::new();
    if let Some(domain) = domain {
        params.insert("domain".to_string(), json!(domain));
    }
    if let Some(ip) = ip {
        params.insert("ip".to_string(), json!(ip));
    }
    Ok(Value::Object(params))
}
