use crate::errors::{AppError, AppResult};
use serde_json::Value;

/// Client-generated DNS IDs remain endpoint-owned even when another settings
/// section references them. Reject removals before publishing, including offline.
pub(super) fn validate(config: &Value) -> AppResult<()> {
    let Some(dns) = config.pointer("/runtime/dns") else {
        return Ok(());
    };
    let mut references = Vec::new();
    references.extend(dns["default_server"].as_str());
    if let Some(dispatch) = dns["dispatch"].as_array() {
        references.extend(dispatch.iter().filter_map(|rule| rule["server"].as_str()));
    }
    let policy = &dns["policy"];
    for key in ["node_server", "direct_server"] {
        references.extend(policy[key].as_str());
    }
    for key in [
        "fallback_servers",
        "node_fallback_servers",
        "direct_fallback_servers",
    ] {
        if let Some(values) = policy[key].as_array() {
            references.extend(values.iter().filter_map(Value::as_str));
        }
    }
    if let Some(timeouts) = policy["server_timeout_ms"].as_object() {
        references.extend(timeouts.keys().map(String::as_str));
    }
    for id in references {
        if id.starts_with("client-endpoint-dns-") && dns["servers"].get(id).is_none() {
            return Err(AppError::invalid_argument(
                "DNS 仍引用已移除或停用的端点服务器，请先调整 DNS 分流和解析策略",
            ));
        }
    }
    Ok(())
}
