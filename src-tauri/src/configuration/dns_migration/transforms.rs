//! Pure transformations of saved DNS values. Never normalizes/serializes a
//! typed model back over the original Value, and never reads dependency files.
use super::*;
use crate::models::dns_config::ClientDnsConfig;

const MAX_SERVERS: usize = 512;
const MAX_REFERENCES: usize = 4096;
fn invalid() -> AppError {
    failure(
        "dns_migration_validation",
        "DNS 已知结构或服务器引用不合法；原材料保留，请先修正 DNS 配置",
    )
}
fn pointer(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}
fn validate_budget(config: &Value) -> AppResult<()> {
    if serde_json::to_vec(config).map_err(|_| invalid())?.len() > MAX_BYTES {
        return Err(failure("dns_migration_budget", "DNS 材料超过16MiB迁移预算"));
    }
    Ok(())
}
fn server_map(config: &Value) -> AppResult<&serde_json::Map<String, Value>> {
    validate_budget(config)?;
    let servers = config
        .get("servers")
        .and_then(Value::as_object)
        .ok_or_else(invalid)?;
    if servers.len() > MAX_SERVERS
        || servers.keys().any(|name| {
            name.trim().is_empty() || name.len() > 512 || name.chars().any(char::is_control)
        })
    {
        return Err(invalid());
    }
    Ok(servers)
}
pub(super) fn references(config: &Value, name: &str) -> AppResult<Vec<Reference>> {
    let mut result = Vec::new();
    let mut add = |path: String, label: &str| {
        result.push(Reference {
            path,
            label: label.into(),
        })
    };
    if config.get("default_server").and_then(Value::as_str) == Some(name) {
        add("/default_server".into(), "默认解析服务器");
    }
    if let Some(dispatch) = config.get("dispatch").and_then(Value::as_array) {
        for (index, rule) in dispatch.iter().enumerate() {
            if rule.get("server").and_then(Value::as_str) == Some(name) {
                add(format!("/dispatch/{index}/server"), "DNS 分流服务器");
            }
        }
    }
    if let Some(policy) = config.get("policy").and_then(Value::as_object) {
        for (field, label) in [
            ("node_server", "节点域名解析服务器"),
            ("direct_server", "直连解析服务器"),
        ] {
            if policy.get(field).and_then(Value::as_str) == Some(name) {
                add(format!("/policy/{field}"), label);
            }
        }
        for (field, label) in [
            ("fallback_servers", "默认有序回退链"),
            ("node_fallback_servers", "节点域名有序回退链"),
            ("direct_fallback_servers", "直连有序回退链"),
        ] {
            for (index, value) in policy
                .get(field)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if value.as_str() == Some(name) {
                    add(format!("/policy/{field}/{index}"), label);
                }
            }
        }
        if policy
            .get("server_timeout_ms")
            .and_then(Value::as_object)
            .is_some_and(|map| map.contains_key(name))
        {
            add(
                format!("/policy/server_timeout_ms/{}", pointer(name)),
                "服务器独立超时键",
            );
        }
    }
    if result.len() > MAX_REFERENCES {
        return Err(failure(
            "dns_migration_budget",
            "DNS 引用超过迁移预算，请分段维护配置",
        ));
    }
    Ok(result)
}
pub(super) fn servers(config: &Value) -> AppResult<Vec<Server>> {
    server_map(config)?
        .keys()
        .map(|name| {
            Ok(Server {
                name: name.clone(),
                references: references(config, name)?,
            })
        })
        .collect()
}
fn contains_name(value: &Value, name: &str) -> bool {
    match value {
        Value::String(value) => value == name,
        Value::Array(values) => values.iter().any(|v| contains_name(v, name)),
        Value::Object(map) => map.iter().any(|(k, v)| k == name || contains_name(v, name)),
        _ => false,
    }
}
fn unknown_fields(value: &Value, known: &[&str], name: &str) -> bool {
    value.as_object().is_some_and(|map| {
        map.iter().any(|(key, value)| {
            !known.contains(&key.as_str()) && (key == name || contains_name(value, name))
        })
    })
}
fn ambiguous(config: &Value, name: &str) -> bool {
    if unknown_fields(
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
        name,
    ) {
        return true;
    }
    for server in config
        .get("servers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|m| m.values())
    {
        // host/bootstrap/detour are endpoint/IP/outbound facts, not references
        // to another DNS backend. Never replace their matching strings.
        if unknown_fields(
            server,
            &[
                "type",
                "host",
                "port",
                "path",
                "bootstrap",
                "server_name",
                "detour",
            ],
            name,
        ) {
            return true;
        }
    }
    for rule in config
        .get("dispatch")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if unknown_fields(rule, &["condition", "server"], name) {
            return true;
        }
    }
    for (field, known) in [
        (
            "policy",
            &[
                "timeout_ms",
                "server_timeout_ms",
                "fallback_servers",
                "node_server",
                "node_fallback_servers",
                "direct_server",
                "direct_fallback_servers",
                "reject_address_cidrs",
                "address_family",
            ][..],
        ),
        ("cache", &["max_entries", "max_ttl_seconds"][..]),
        (
            "reverse_mapping",
            &["max_entries", "max_domains_per_address", "max_ttl_seconds"][..],
        ),
        (
            "answer",
            &[
                "type",
                "cidr",
                "ipv6_cidr",
                "ttl_seconds",
                "max_entries",
                "exclude_domains",
            ][..],
        ),
    ] {
        if config
            .get(field)
            .is_some_and(|v| unknown_fields(v, known, name))
        {
            return true;
        }
    }
    false
}
fn validate(config: &Value) -> AppResult<()> {
    let client: ClientDnsConfig = serde_json::from_value(config.clone()).map_err(|_| invalid())?;
    // This includes all three independent chains and timeout keys even when
    // the wrapper's enabled flag is false. Do not drop the config on disable.
    client.validate_client_shape().map_err(|_| invalid())?;
    // Deserialization only: no RuntimeConfig::parse/validate/compile or IO.
    // Unsupported extension fields remain in the saved material; they are not
    // stripped merely to manufacture a valid native candidate.
    serde_json::from_value::<zero_config::DnsConfig>(config.clone()).map_err(|_| invalid())?;
    Ok(())
}
pub(super) fn migrate(
    config: &Value,
    name: &str,
    replacement: Option<&str>,
) -> AppResult<(Value, Vec<Reference>)> {
    let servers = server_map(config)?;
    if !servers.contains_key(name) || servers.len() < 2 {
        return Err(failure(
            "dns_migration_target",
            "删除目标不存在或已是最后一个 DNS 服务器；请重新读取并保留至少一个服务器",
        ));
    }
    let references = references(config, name)?;
    if replacement.is_some_and(|target| target == name || !servers.contains_key(target))
        || (!references.is_empty() && replacement.is_none())
    {
        return Err(failure(
            "dns_migration_replacement",
            "请显式选择一个现存且不同的替代服务器，迁移全部引用后再删除",
        ));
    }
    if ambiguous(config, name) {
        return Err(failure("dns_migration_ambiguous","未知 DNS 扩展含同名材料，不能证明其引用语义；原材料保留，请先在来源或完整配置中明确处理"));
    }
    let mut next = config.clone();
    if let Some(target) = replacement {
        // Update only the original, enumerated semantic locations, never a
        // recursive string replacement that could alter host names or domains.
        for reference in &references {
            if reference.path.starts_with("/policy/server_timeout_ms/") {
                continue;
            }
            *next.pointer_mut(&reference.path).ok_or_else(invalid)? = Value::String(target.into());
        }
        if let Some(timeouts) = next
            .pointer_mut("/policy/server_timeout_ms")
            .and_then(Value::as_object_mut)
        {
            if timeouts.contains_key(name) {
                if timeouts.contains_key(target) {
                    return Err(failure(
                        "dns_migration_timeout_conflict",
                        "替代服务器已有独立超时；不能静默覆盖，请先明确调整超时配置",
                    ));
                }
                let value = timeouts.remove(name).ok_or_else(invalid)?;
                timeouts.insert(target.into(), value);
            }
        }
    }
    next.get_mut("servers")
        .and_then(Value::as_object_mut)
        .ok_or_else(invalid)?
        .remove(name)
        .ok_or_else(invalid)?;
    // Replacement-induced duplicate/primary fallback conflicts are rejected,
    // never silently deduplicated or reordered.
    validate(&next)?;
    Ok((next, references))
}

/// Rename one definition and its complete, enumerated DNS references. Keep the
/// original Value for every other field; never serialize a reduced form model.
pub(super) fn rename(
    config: &Value,
    name: &str,
    target: &str,
) -> AppResult<(Value, Vec<Reference>)> {
    let servers = server_map(config)?;
    if !servers.contains_key(name) {
        return Err(failure(
            "dns_migration_target",
            "原 DNS 服务器已不存在，请重新读取",
        ));
    }
    if target.is_empty()
        || target != target.trim()
        || target.len() > 512
        || target.chars().any(char::is_control)
        || target == name
        || servers.contains_key(target)
    {
        return Err(failure(
            "dns_migration_name",
            "新名称须为不同且未占用的有效名称，不含首尾空白或控制字符，最多512字节",
        ));
    }
    if config
        .pointer("/policy/server_timeout_ms")
        .and_then(Value::as_object)
        .is_some_and(|timeouts| timeouts.contains_key(target))
    {
        return Err(failure(
            "dns_migration_timeout_conflict",
            "新名称已有独立超时键；不能覆盖或借重命名修复失效引用",
        ));
    }
    if ambiguous(config, name) || ambiguous(config, target) {
        return Err(failure(
            "dns_migration_ambiguous",
            "未知 DNS 扩展含原名或新名，无法证明引用语义；原材料保留",
        ));
    }
    validate(config)?;
    let references = references(config, name)?;
    let mut next = config.clone();
    for reference in &references {
        if !reference.path.starts_with("/policy/server_timeout_ms/") {
            *next.pointer_mut(&reference.path).ok_or_else(invalid)? = Value::String(target.into());
        }
    }
    if let Some(timeouts) = next
        .pointer_mut("/policy/server_timeout_ms")
        .and_then(Value::as_object_mut)
    {
        if let Some(value) = timeouts.remove(name) {
            timeouts.insert(target.into(), value);
        }
    }
    let definitions = next
        .get_mut("servers")
        .and_then(Value::as_object_mut)
        .ok_or_else(invalid)?;
    let original = definitions.remove(name).ok_or_else(invalid)?;
    definitions.insert(target.into(), original);
    // Expanded names must remain inside the same material budget. Validate
    // every chain and timeout reference, including retained disabled DNS.
    server_map(&next)?;
    validate(&next)?;
    if !self::references(&next, name)?.is_empty() {
        return Err(invalid());
    }
    Ok((next, references))
}
