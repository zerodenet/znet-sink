//! Global user-owned endpoint definitions. Never mutate subscription/source material.
use crate::errors::{AppError, AppResult};
use serde_json::Value;
use std::collections::HashSet;

fn invalid(message: &str) -> AppError {
    AppError::invalid_argument(message)
}
#[path = "user_endpoints/wireguard.rs"]
mod wireguard;
struct ProtocolAdapter {
    kind: &'static str,
    label: &'static str,
    validate: fn(&Value) -> AppResult<()>,
}
// Register only implemented client authoring adapters. Kernel acceptance is
// checked independently at application; no future protocol is advertised early.
const PROTOCOLS: &[ProtocolAdapter] = &[ProtocolAdapter {
    kind: "wireguard",
    label: "WireGuard",
    validate: wireguard::validate,
}];
pub(crate) fn protocols() -> Vec<Value> {
    PROTOCOLS
        .iter()
        .map(|adapter| serde_json::json!({"type": adapter.kind, "label": adapter.label}))
        .collect()
}
pub(crate) fn validate(rows: &[Value]) -> AppResult<()> {
    if rows.len() > 256 {
        return Err(invalid("最多保存 256 个用户端点"));
    }
    let mut tags = HashSet::new();
    for row in rows {
        let tag = row["tag"]
            .as_str()
            .filter(|s| !s.is_empty() && s.trim() == *s && s.len() <= 128)
            .ok_or_else(|| invalid("端点名称不能为空、超过 128 字节或包含首尾空格"))?;
        if !tags.insert(tag) {
            return Err(invalid("用户端点名称不能重复"));
        }
        if row.get("enabled").is_some_and(|v| !v.is_boolean()) {
            return Err(invalid("端点 enabled 必须为布尔值"));
        }
        if let Some(directions) = row.get("directions") {
            if !directions["inbound"].is_boolean()
                || !directions["outbound"].is_boolean()
                || (directions["inbound"] == false && directions["outbound"] == false)
            {
                return Err(invalid("端点必须启用至少一个方向"));
            }
        }
        let protocol = &row["protocol"];
        let adapter = PROTOCOLS
            .iter()
            .find(|adapter| protocol["type"].as_str() == Some(adapter.kind))
            .ok_or_else(|| invalid("当前客户端尚不支持此端点协议的创建或编辑"))?;
        (adapter.validate)(protocol)?;
    }
    Ok(())
}

// Internal callers sometimes resolve an already resolved source. Identical
// protocol definitions coalesce; differing definitions must never be overwritten.
pub(crate) fn merge(source: &mut Value, rows: &[Value]) -> AppResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    validate(rows)?;
    let object = source
        .as_object_mut()
        .ok_or_else(|| invalid("配置必须为对象"))?;
    for row in rows {
        let tag = &row["tag"];
        for section in ["inbounds", "outbounds", "outbound_groups"] {
            if object
                .get(section)
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().any(|v| v["tag"] == *tag))
            {
                return Err(invalid(&format!(
                    "用户端点 {} 与配置中的 {section} 名称冲突，请重命名用户端点",
                    tag.as_str().unwrap()
                )));
            }
        }
    }
    let entries = object
        .entry("endpoints")
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut()
        .ok_or_else(|| invalid("配置 endpoints 必须为数组"))?;
    for row in rows {
        let matches: Vec<_> = entries
            .iter()
            .enumerate()
            .filter(|(_, v)| v["tag"] == row["tag"])
            .map(|(i, _)| i)
            .collect();
        match matches.as_slice() {
            [] => entries.push(row.clone()),
            [index] if same_definition(&entries[*index], row) => entries[*index] = row.clone(),
            _ => {
                return Err(invalid(&format!(
                    "用户端点 {} 与配置端点名称冲突，请重命名用户端点",
                    row["tag"].as_str().unwrap()
                )))
            }
        }
    }
    Ok(())
}
fn same_definition(left: &Value, right: &Value) -> bool {
    let strip = |v: &Value| {
        let mut v = v.clone();
        if let Some(v) = v.as_object_mut() {
            v.remove("enabled");
            v.remove("directions");
        }
        v
    };
    strip(left) == strip(right)
}

/// Reject removal/rename while a stored profile still explicitly references it.
pub(crate) fn check_removed(previous: &[Value], next: &[Value], source: &Value) -> AppResult<()> {
    for row in previous {
        let tag = row["tag"].as_str().unwrap_or("");
        if next.iter().any(|v| v["tag"] == tag) {
            continue;
        }
        if ["endpoints", "outbounds", "outbound_groups"]
            .iter()
            .any(|key| {
                source[*key]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|v| v["tag"] == tag))
            })
        {
            continue;
        }
        if references(source, tag) {
            return Err(invalid(&format!(
                "端点 {tag} 仍被配置的策略、路由或 DNS 引用，请先调整引用"
            )));
        }
    }
    Ok(())
}
fn references(value: &Value, tag: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (matches!(key.as_str(), "outbound" | "detour" | "selected" | "default")
                && value.as_str() == Some(tag))
                || (key == "outbounds"
                    && value
                        .as_array()
                        .is_some_and(|rows| rows.iter().any(|v| v.as_str() == Some(tag))))
                || references(value, tag)
        }),
        Value::Array(rows) => rows.iter().any(|v| references(v, tag)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde_json::json;
    #[test]
    fn only_implemented_protocols_are_advertised_and_unknown_protocols_are_rejected() {
        assert_eq!(
            protocols(),
            vec![serde_json::json!({"type":"wireguard","label":"WireGuard"})]
        );
        let mut future = endpoint();
        future["protocol"] = serde_json::json!({"type":"future-mesh","options":{"nested":true}});
        assert!(validate(&[future]).is_err());
    }
    fn endpoint() -> Value {
        json!({"tag":"my-wg","enabled":true,"directions":{"inbound":false,"outbound":true},"protocol":{"type":"wireguard","private_key":STANDARD.encode([1;32]),"addresses":["10.2.0.1/32"],"peers":[{"public_key":STANDARD.encode([2;32]),"endpoint":"vpn.example:51820","allowed_ips":["0.0.0.0/0","::/0"]}]}})
    }
    #[test]
    fn endpoint_library_survives_serialization_and_merges_every_source_without_mutation() {
        let rows: Vec<Value> =
            serde_json::from_str(&serde_json::to_string(&vec![endpoint()]).unwrap()).unwrap();
        for source in [
            json!({}),
            json!({"endpoints":[]}),
            json!({"endpoints":[{"tag":"subscription","protocol":{"type":"future"}}]}),
        ] {
            let original = source.clone();
            let mut effective = source.clone();
            merge(&mut effective, &rows).unwrap();
            assert_eq!(
                effective["endpoints"].as_array().unwrap().last().unwrap(),
                &endpoint()
            );
            let first = effective.clone();
            merge(&mut effective, &rows).unwrap();
            assert_eq!(first, effective);
            assert_eq!(source, original);
            // Deletion from the global library must not leave a definition in the source.
            let mut deleted = source.clone();
            merge(&mut deleted, &[]).unwrap();
            assert_eq!(deleted, source);
        }
    }
    #[test]
    fn conflicts_and_invalid_keys_are_rejected_without_secret_errors() {
        let row = endpoint();
        let mut source = json!({"endpoints":[{"tag":"my-wg","protocol":{"type":"future"}}]});
        assert!(merge(&mut source, &[row.clone()]).is_err());
        for key in ["inbounds", "outbounds", "outbound_groups"] {
            assert!(merge(&mut json!({key:[{"tag":"my-wg"}]}), &[row.clone()]).is_err());
        }
        assert!(validate(&[row.clone(), row.clone()]).is_err());
        let mut invalid_row = row;
        invalid_row["protocol"]["private_key"] = json!("secret-invalid");
        assert!(!validate(&[invalid_row])
            .unwrap_err()
            .message
            .contains("secret-invalid"));
    }
    #[test]
    fn invalid_addresses_directions_peers_and_ports_are_rejected() {
        for (path, value) in [
            ("/protocol/addresses", json!(["10.0.0.1/33"])),
            ("/protocol/peers", json!([])),
            ("/directions", json!({"inbound":false,"outbound":false})),
            ("/protocol/peers/0/allowed_ips", json!(["::/129"])),
            ("/protocol/peers/0/endpoint", json!("vpn.test:0")),
        ] {
            let mut row = endpoint();
            *row.pointer_mut(path).unwrap() = value;
            assert!(validate(&[row]).is_err(), "{path}");
        }
        let mut inbound = endpoint();
        inbound["protocol"]["peers"][0]
            .as_object_mut()
            .unwrap()
            .remove("endpoint");
        assert!(validate(&[inbound]).is_ok());
    }
    #[test]
    fn deleting_or_renaming_referenced_endpoints_is_rejected_even_offline() {
        for source in [
            json!({"route":{"final":{"outbound":"my-wg"}}}),
            json!({"outbound_groups":[{"tag":"selector","outbounds":["my-wg"]}]}),
            json!({"runtime":{"dns":{"servers":[{"detour":"my-wg"}]}}}),
        ] {
            assert!(check_removed(&[endpoint()], &[], &source).is_err());
            assert!(check_removed(&[endpoint()], &[endpoint()], &source).is_ok());
        }
        assert!(check_removed(
            &[endpoint()],
            &[],
            &json!({"outbounds":[{"tag":"my-wg"}],"route":{"final":{"outbound":"my-wg"}}})
        )
        .is_ok());
    }
}
