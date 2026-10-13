use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppDnsConfig};
use serde_json::Value;
use std::net::IpAddr;

pub const FIELDS: &[&str] = &[
    "localProxy.host",
    "localProxy.port",
    "urlTest.url",
    "urlTest.toleranceMs",
    "runtime.udpUpstreamIdleTimeoutSeconds",
    "tun.tag",
    "tun.addr",
    "tun.secondaryAddr",
    "tun.mtu",
    "tun.includeCidrs",
    "tun.excludeCidrs",
    "tun.dualStack",
    "tun.dnsHijack",
    "routing.injectCommonRules",
    "dns",
    "bypass",
];

pub fn invalid(field: &str) -> AppError {
    AppError {
        code: "invalid_argument",
        message: "设置迁移字段不支持、格式错误或包含不可迁移的敏感材料".into(),
        details: Some(serde_json::json!({"fieldPath":field})),
    }
}

pub fn validate_field(key: &str, value: &Value, current: &AppConfig) -> AppResult<()> {
    if !FIELDS.contains(&key) {
        return Err(invalid("clientEdits"));
    }
    // Reuse shared local-settings validators, with no mutation of the source/profile.
    crate::configuration::local_edits::validate_client_edits(
        current,
        &std::collections::BTreeMap::from([(key.to_owned(), value.clone())]),
    )
    .map_err(|_| invalid(key))?;
    match key {
        "urlTest.url" => portable_url(value.as_str().ok_or_else(|| invalid(key))?, key)?,
        "localProxy.host" => host(value.as_str().ok_or_else(|| invalid(key))?, key)?,
        "tun.addr" | "tun.secondaryAddr" => {
            if !value.is_null() || key == "tun.addr" {
                crate::services::kernel_settings::validate_cidr(
                    value.as_str().ok_or_else(|| invalid(key))?,
                    key,
                )
                .map_err(|_| invalid(key))?;
            }
        }
        "localProxy.port" => {
            if value.as_u64().is_none_or(|v| !(1..=65535).contains(&v)) {
                return Err(invalid(key));
            }
        }
        "tun.dualStack" | "tun.dnsHijack" | "routing.injectCommonRules" => {
            if !value.is_boolean() {
                return Err(invalid(key));
            }
        }
        "tun.includeCidrs" | "tun.excludeCidrs" => {
            if value
                .as_array()
                .is_none_or(|items| items.iter().any(|v| !v.is_string()))
            {
                return Err(invalid(key));
            }
        }
        "tun.tag" => {
            if value.as_str().is_none_or(|v| v.trim().is_empty()) {
                return Err(invalid(key));
            }
        }
        "tun.mtu" => {
            if value.as_u64().is_none_or(|v| !(576..=65535).contains(&v)) {
                return Err(invalid(key));
            }
        }
        "runtime.udpUpstreamIdleTimeoutSeconds" => {
            if value
                .as_u64()
                .is_none_or(|v| v == 0 || v > 9_007_199_254_740_991)
            {
                return Err(invalid(key));
            }
        }
        "urlTest.toleranceMs" => {
            if value.as_u64().is_none_or(|v| v > 9_007_199_254_740_991) {
                return Err(invalid(key));
            }
        }
        "dns" => dns(value)?,
        "bypass" => {
            allowed(value, &["rules", "localNetworks"], key)?;
            if let Some(rules) = value.get("rules").and_then(Value::as_array) {
                for rule in rules {
                    let raw = rule.as_str().ok_or_else(|| invalid(key))?;
                    if raw.contains(['@', '?', '#']) || raw.contains("://") {
                        return Err(invalid(key));
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn portable_url(raw: &str, field: &str) -> AppResult<()> {
    let url = reqwest::Url::parse(raw).map_err(|_| invalid(field))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(field));
    }
    // The migration contract cannot classify arbitrary private paths as credential-free.
    let builtin = reqwest::Url::parse(&crate::models::app_config::default_url_test_url())
        .map_err(|_| invalid(field))?;
    if url.path() != "/" && url.path() != builtin.path() {
        return Err(invalid(field));
    }
    Ok(())
}

fn host(raw: &str, field: &str) -> AppResult<()> {
    if raw.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if raw.is_empty()
        || raw.contains(['/', '@', '?', '#', '%', ':'])
        || raw.chars().any(char::is_whitespace)
    {
        return Err(invalid(field));
    }
    let parsed = reqwest::Url::parse(&format!("https://{raw}/")).map_err(|_| invalid(field))?;
    if parsed.host_str().is_none() {
        return Err(invalid(field));
    }
    Ok(())
}

fn allowed(value: &Value, fields: &[&str], path: &str) -> AppResult<()> {
    let object = value.as_object().ok_or_else(|| invalid(path))?;
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err(invalid(path));
    }
    Ok(())
}

/// Reject fields ignored by serde unit variants without maintaining another DNS schema.
fn retains_input_fields(input: &Value, canonical: &Value) -> AppResult<()> {
    match input {
        Value::Object(fields) => {
            let known = canonical.as_object().ok_or_else(|| invalid("dns.config"))?;
            for (field, value) in fields {
                let canonical = known.get(field).ok_or_else(|| invalid("dns.config"))?;
                retains_input_fields(value, canonical)?;
            }
        }
        Value::Array(values) => {
            let known = canonical
                .as_array()
                .filter(|v| v.len() == values.len())
                .ok_or_else(|| invalid("dns.config"))?;
            for (input, canonical) in values.iter().zip(known) {
                retains_input_fields(input, canonical)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn public_dns_host(raw: &str) -> AppResult<()> {
    host(raw, "dns.config.servers.host")?;
    if raw.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    // These hostnames come exclusively from shipped public presets, never user settings.
    let presets =
        serde_json::to_value(crate::models::dns_config::ClientDnsConfig::recommended_default())
            .map_err(|_| invalid("dns.config.servers.host"))?;
    let public = presets["servers"].as_object().is_some_and(|servers| {
        servers.values().any(|server| {
            server
                .get("host")
                .and_then(Value::as_str)
                .is_some_and(|host| host.eq_ignore_ascii_case(raw))
        })
    });
    if !public {
        return Err(invalid("dns.config.servers.host"));
    }
    Ok(())
}

fn dns(value: &Value) -> AppResult<()> {
    allowed(value, &["enabled", "config", "dnsHijack"], "dns")?;
    let dns: AppDnsConfig = serde_json::from_value(value.clone()).map_err(|_| invalid("dns"))?;
    if let Some(config) = value.get("config").filter(|v| !v.is_null()) {
        // Native Zero deserialization rejects unknown fields and invalid matcher schemas.
        let native: zero_config::DnsConfig =
            serde_json::from_value(config.clone()).map_err(|_| invalid("dns.config"))?;
        let canonical = serde_json::to_value(native).map_err(|_| invalid("dns.config"))?;
        retains_input_fields(config, &canonical)?;
        allowed(
            config,
            &[
                "servers",
                "default_server",
                "dispatch",
                "cache",
                "reverse_mapping",
                "answer",
                "policy",
            ],
            "dns.config",
        )?;
        let servers = config["servers"]
            .as_object()
            .ok_or_else(|| invalid("dns.config.servers"))?;
        for server in servers.values() {
            for field in ["host", "server_name"] {
                if let Some(raw) = server.get(field).filter(|value| !value.is_null()) {
                    let raw = raw.as_str().ok_or_else(|| invalid("dns.config.servers"))?;
                    public_dns_host(raw)?;
                    if field == "server_name" {
                        if let Some(upstream) = server
                            .get("host")
                            .and_then(Value::as_str)
                            .filter(|host| host.parse::<IpAddr>().is_err())
                        {
                            if !upstream.eq_ignore_ascii_case(raw) {
                                return Err(invalid("dns.config.servers.server_name"));
                            }
                        }
                    }
                }
            }
            if let Some(path) = server.get("path") {
                if path.as_str() != Some("/dns-query") {
                    return Err(invalid("dns.config.servers.path"));
                }
            }
        }
        dns.config
            .as_ref()
            .ok_or_else(|| invalid("dns.config"))?
            .validate_client_shape()
            .map_err(|_| invalid("dns.config"))?;
    } else if dns.enabled {
        return Err(invalid("dns.config"));
    }
    Ok(())
}
