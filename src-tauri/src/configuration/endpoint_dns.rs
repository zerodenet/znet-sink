//! Client-only imported DNS preferences, projected into the existing DNS contract.
use crate::errors::{AppError, AppResult};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::IpAddr;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    servers: Vec<IpAddr>,
    #[serde(default)]
    search_domains: Vec<String>,
    #[serde(default)]
    use_as_default: bool,
}
fn parse(row: &Value) -> AppResult<Option<Preferences>> {
    row.get("dns")
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|_| {
                AppError::invalid_argument("端点 DNS 配置无效，请填写 DNS 服务器 IP 和有效搜索域")
            })
        })
        .transpose()
}
pub(crate) fn validate(rows: &[Value]) -> AppResult<()> {
    let mut defaults = 0;
    for row in rows {
        let Some(dns) = parse(row)? else { continue };
        if dns.servers.is_empty()
            || dns.servers.len() > 16
            || dns.search_domains.len() > 32
            || dns.search_domains.iter().any(|name| !valid_domain(name))
        {
            return Err(AppError::invalid_argument(
                "端点 DNS 需要 1～16 个服务器 IP；搜索域最多 32 个且必须为有效域名",
            ));
        }
        defaults += usize::from(dns.use_as_default);
    }
    if defaults > 1 {
        return Err(AppError::invalid_argument(
            "只能选择一个端点作为默认 DNS 来源",
        ));
    }
    Ok(())
}
fn valid_domain(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
pub(crate) fn kernel_endpoint(row: &Value) -> Value {
    let mut row = row.clone();
    if let Some(row) = row.as_object_mut() {
        row.remove("dns");
    }
    row
}
pub(crate) fn owns_dns(rows: &[Value], tag: &Value) -> bool {
    rows.iter()
        .any(|row| row["tag"] == *tag && row.get("dns").is_some())
}
fn server_tag(tag: &str, index: usize) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "client-endpoint-dns-{:x}-{index}",
        Sha256::digest(tag.as_bytes())
    )
}
pub(crate) fn apply(config: &mut Value, rows: &[Value]) -> AppResult<()> {
    validate(rows)?;
    let active: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["enabled"] != false
                && row["directions"]["outbound"] != false
                && row.get("dns").is_some()
        })
        .collect();
    if active.is_empty() {
        return Ok(());
    }
    let has_default = active
        .iter()
        .any(|row| row.pointer("/dns/use_as_default") == Some(&Value::Bool(true)));
    if config.pointer("/runtime/dns").is_none() && !has_default {
        return Ok(());
    }
    let runtime = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("配置必须为对象"))?
        .entry("runtime")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("runtime 必须为对象"))?;
    let dns = runtime
        .entry("dns")
        .or_insert_with(|| json!({"servers":{},"default_server":"client-endpoint-bootstrap"}));
    if dns["default_server"].as_str().is_none() {
        return Err(AppError::invalid_argument("DNS 缺少默认服务器"));
    }
    let mut default = None;
    for row in active {
        let preference = parse(row)?.unwrap();
        let tag = row["tag"]
            .as_str()
            .ok_or_else(|| AppError::invalid_argument("端点名称无效"))?;
        for (index, address) in preference.servers.iter().enumerate() {
            let name = server_tag(tag, index);
            let mut value = json!({"type":"udp","host":address.to_string(),"port":53});
            if super::user_endpoints::dns_uses_endpoint(row, *address) {
                value["detour"] = json!(tag);
            }
            let servers = dns["servers"]
                .as_object_mut()
                .ok_or_else(|| AppError::invalid_argument("DNS 服务器必须为对象"))?;
            if servers.get(&name).is_some_and(|old| *old != value) {
                return Err(AppError::invalid_argument(
                    "导入 DNS 与配置中的服务器名称冲突",
                ));
            }
            servers.insert(name.clone(), value);
            if preference.use_as_default && index == 0 {
                default = Some(name);
            }
        }
    }
    if let Some(default) = default {
        dns.as_object_mut()
            .ok_or_else(|| AppError::invalid_argument("DNS 必须为对象"))?
            .entry("policy")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_argument("DNS policy 必须为对象"))?;
        // Keep node bootstrap independent of the endpoint it is resolving.
        // Prefer an existing direct literal server, then the client's normal
        // recommended bootstrap. Never introduce a system-DNS capture loop.
        let explicit_node = dns
            .pointer("/policy/node_server")
            .and_then(Value::as_str)
            .is_some();
        if !explicit_node {
            let independent = dns["servers"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, server)| {
                    server["type"] != "system"
                        && server.get("detour").is_none_or(Value::is_null)
                        && server["host"]
                            .as_str()
                            .is_some_and(|host| host.parse::<IpAddr>().is_ok())
                })
                .map(|(tag, _)| tag.clone());
            let node = if let Some(tag) = independent {
                tag
            } else {
                let recommended = serde_json::to_value(
                    crate::models::dns_config::ClientDnsConfig::recommended_default(),
                )
                .map_err(|_| AppError::internal("DNS 引导配置生成失败"))?;
                let tag = recommended["policy"]["node_server"]
                    .as_str()
                    .ok_or_else(|| AppError::internal("缺少独立 DNS 引导配置"))?;
                let value = recommended["servers"][tag].clone();
                let servers = dns["servers"].as_object_mut().unwrap();
                if servers
                    .get("client-endpoint-bootstrap")
                    .is_some_and(|old| *old != value)
                {
                    return Err(AppError::invalid_argument("端点 DNS 引导服务器名称冲突"));
                }
                servers.insert("client-endpoint-bootstrap".into(), value);
                "client-endpoint-bootstrap".to_owned()
            };
            let policy = dns
                .as_object_mut()
                .unwrap()
                .entry("policy")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| AppError::invalid_argument("DNS policy 必须为对象"))?;
            policy.insert("node_server".into(), json!(node));
        }
        dns["default_server"] = json!(default);
        // Other DNS addresses from the same selected endpoint are ordered
        // fallbacks. Never leak a private lookup to unrelated imported endpoints.
        let selected = rows
            .iter()
            .find(|row| row.pointer("/dns/use_as_default") == Some(&Value::Bool(true)))
            .unwrap();
        let count = parse(selected)?.unwrap().servers.len();
        let tag = selected["tag"].as_str().unwrap();
        dns["policy"]["fallback_servers"] =
            json!((1..count).map(|i| server_tag(tag, i)).collect::<Vec<_>>());
    }
    // Preserve the configured default and dispatch when the imported endpoint
    // is not the selected DNS source. Its resolvers remain selectable.
    Ok(())
}

#[cfg(test)]
#[path = "endpoint_dns_tests.rs"]
mod tests;
