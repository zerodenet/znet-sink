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
    if limit.is_some_and(|limit| !(1..=500).contains(&limit)) {
        return Err(crate::errors::AppError::invalid_argument(
            "DNS 缓存读取上限必须为1–500",
        ));
    }
    let mut params = Map::new();
    if let Some(domain) = normalize_optional(domain) {
        params.insert("domain".to_string(), json!(domain));
    }
    if let Some(limit) = limit {
        params.insert("limit".to_string(), json!(limit));
    }
    let value = run_command("diagnostics.dns_cache", Value::Object(params), options).await?;
    cache_command_result(value)
}

fn cache_command_result(value: Value) -> AppResult<Value> {
    if value.get("accepted").and_then(Value::as_bool).is_none() {
        return Err(cache_invalid());
    }
    let response: zero_api::CommandResponse =
        serde_json::from_value(value).map_err(|_| cache_invalid())?;
    if !response.accepted {
        return Err(crate::errors::AppError {
            code: "core_error",
            message: "内核未接受 DNS 缓存读取；没有确认缓存状态".into(),
            details: None,
        });
    }
    cache_result(response.result.ok_or_else(cache_invalid)?)
}

fn cache_invalid() -> crate::errors::AppError {
    crate::errors::AppError {
        code: "invalid_response",
        message: "DNS 缓存结果不完整或格式未适配；没有推断为空缓存或零计数".into(),
        details: None,
    }
}
fn cache_result(value: Value) -> AppResult<Value> {
    let enabled = value["enabled"].as_bool().ok_or_else(cache_invalid)?;
    let addresses = |v: &Value| -> AppResult<Value> {
        let items = v
            .as_array()
            .filter(|v| v.len() <= 500)
            .ok_or_else(cache_invalid)?;
        if items.iter().any(|v| {
            v.as_str()
                .is_none_or(|s| s.parse::<std::net::IpAddr>().is_err())
        }) {
            return Err(cache_invalid());
        }
        Ok(json!(items))
    };
    let ttl = |v: &Value| -> AppResult<Value> {
        match v {
            Value::Null => Ok(Value::Null),
            _ => v
                .as_u64()
                .map(|n| json!(n.to_string()))
                .ok_or_else(cache_invalid),
        }
    };
    if let Some(domain) = value.get("domain") {
        let domain = domain
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 4096)
            .ok_or_else(cache_invalid)?;
        let hit = value["hit"].as_bool().ok_or_else(cache_invalid)?;
        let list = addresses(&value["addresses"])?;
        let ttl_value = value.get("ttl_seconds").ok_or_else(cache_invalid)?;
        if hit && ttl_value.is_null() {
            return Err(cache_invalid());
        }
        if !hit && (!list.as_array().is_some_and(Vec::is_empty) || !ttl_value.is_null()) {
            return Err(cache_invalid());
        }
        return Ok(
            json!({"enabled":enabled,"domain":domain,"hit":hit,"addresses":list,"ttl_seconds":ttl(ttl_value)?}),
        );
    }
    let entries = value["entries"]
        .as_array()
        .filter(|v| v.len() <= 500)
        .ok_or_else(cache_invalid)?;
    let count = value["count"].as_u64().ok_or_else(cache_invalid)?;
    if count != entries.len() as u64 {
        return Err(cache_invalid());
    }
    let entries = entries.iter().map(|entry| {
        let domain = entry["domain"].as_str().filter(|s| !s.is_empty() && s.len() <= 4096).ok_or_else(cache_invalid)?;
        let ttl_value = entry.get("ttl_seconds").filter(|v| !v.is_null()).ok_or_else(cache_invalid)?;
        Ok(json!({"domain":domain,"addresses":addresses(&entry["addresses"])? ,"ttl_seconds":ttl(ttl_value)?}))
    }).collect::<AppResult<Vec<Value>>>()?;
    Ok(json!({"enabled":enabled,"entries":entries,"count":count}))
}

#[cfg(test)]
#[path = "dns_cache_tests.rs"]
mod cache_tests;

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
