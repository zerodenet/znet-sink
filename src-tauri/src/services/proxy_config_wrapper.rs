#[path = "proxy_config.rs"]
mod original;

pub use original::*;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::errors::{AppError, AppResult};
use crate::models::proxy_config::{ProxyConfigProfile, ProxyConfigUpsert};
use crate::services::common::lock;
use crate::state::app_state::AppState;

const MANAGED_MIXED_TAG: &str = "znet-sink-mixed-in";
const LEGACY_MANAGED_MIXED_TAG: &str = "mixed-in";

fn is_subscription_source(input: &ProxyConfigUpsert) -> bool {
    input
        .path
        .as_deref()
        .map(str::trim)
        .is_some_and(|path| path.starts_with("https://") || path.starts_with("http://"))
}

fn local_protocol(inbound: &Value) -> Option<&str> {
    inbound
        .get("protocol")
        .and_then(|protocol| protocol.get("type"))
        .and_then(Value::as_str)
}

fn is_local_proxy_inbound(inbound: &Value) -> bool {
    local_protocol(inbound).is_some_and(|protocol| {
        matches!(
            protocol.trim().to_ascii_lowercase().as_str(),
            "mixed" | "http" | "socks5"
        )
    })
}

fn is_managed_local_inbound(inbound: &Value) -> bool {
    inbound
        .get("tag")
        .and_then(Value::as_str)
        .is_some_and(|tag| {
            matches!(tag.trim(), MANAGED_MIXED_TAG | LEGACY_MANAGED_MIXED_TAG)
        })
}

fn local_inbound_is_usable(inbound: &Value) -> bool {
    original::extract_local_proxy(&json!({ "inbounds": [inbound.clone()] })).is_some()
}

fn set_managed_endpoint(inbound: &mut Value, host: &str, port: u16) -> AppResult<()> {
    let object = inbound.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("subscription local inbound must be an object")
    })?;
    if object
        .get("tag")
        .and_then(Value::as_str)
        .is_none_or(|tag| tag.trim().is_empty())
    {
        object.insert(
            "tag".to_string(),
            Value::String(MANAGED_MIXED_TAG.to_string()),
        );
    }
    let listen = object
        .entry("listen".to_string())
        .or_insert_with(|| json!({}));
    let listen = listen.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("subscription local inbound listen must be an object")
    })?;
    listen.insert("address".to_string(), Value::String(host.to_string()));
    listen.insert("port".to_string(), json!(port));
    Ok(())
}

fn ensure_subscription_local_inbound(
    content: &mut Value,
    existing_content: Option<&Value>,
    host: &str,
    port: u16,
) -> AppResult<()> {
    let object = content.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("subscription must produce a JSON object")
    })?;
    if !object.contains_key("inbounds") {
        object.insert("inbounds".to_string(), Value::Array(Vec::new()));
    }
    let inbounds = object
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::invalid_argument("subscription inbounds must be an array"))?;

    let mut incomplete_local_index = None;
    for (index, inbound) in inbounds.iter_mut().enumerate() {
        if !is_local_proxy_inbound(inbound) {
            continue;
        }
        if is_managed_local_inbound(inbound) {
            set_managed_endpoint(inbound, host, port)?;
            return Ok(());
        }
        if local_inbound_is_usable(inbound) {
            return Ok(());
        }
        incomplete_local_index.get_or_insert(index);
    }

    if let Some(index) = incomplete_local_index {
        set_managed_endpoint(&mut inbounds[index], host, port)?;
        return Ok(());
    }

    if let Some(mut inbound) = existing_content
        .and_then(|existing| existing.get("inbounds"))
        .and_then(Value::as_array)
        .and_then(|existing| {
            existing
                .iter()
                .find(|inbound| is_local_proxy_inbound(inbound))
                .cloned()
        })
    {
        if is_managed_local_inbound(&inbound) || !local_inbound_is_usable(&inbound) {
            set_managed_endpoint(&mut inbound, host, port)?;
        }
        inbounds.push(inbound);
        return Ok(());
    }

    inbounds.push(json!({
        "tag": MANAGED_MIXED_TAG,
        "listen": { "address": host, "port": port },
        "protocol": { "type": "mixed" }
    }));
    Ok(())
}

fn prepare_subscription_upsert(state: &AppState, input: &mut ProxyConfigUpsert) -> AppResult<()> {
    if !is_subscription_source(input) {
        return Ok(());
    }

    let (host, port) = {
        let config = lock(state.app_config(), "app_config")?;
        (config.local_proxy.host.clone(), config.local_proxy.port)
    };
    let existing_content = input.id.as_ref().and_then(|id| {
        lock(state.proxy_configs(), "proxy_config")
            .ok()
            .and_then(|profiles| {
                profiles
                    .iter()
                    .find(|profile| profile.id == *id)
                    .and_then(|profile| profile.content.clone())
            })
    });

    if let Some(content) = input.content.as_mut() {
        ensure_subscription_local_inbound(content, existing_content.as_ref(), &host, port)?;
    }
    Ok(())
}

pub async fn upsert_runtime(
    app_handle: AppHandle,
    mut input: ProxyConfigUpsert,
) -> AppResult<ProxyConfigProfile> {
    {
        let state = app_handle.state::<AppState>();
        prepare_subscription_upsert(state.inner(), &mut input)?;
    }
    original::upsert_runtime(app_handle, input).await
}

#[cfg(test)]
mod wrapper_tests {
    use super::{ensure_subscription_local_inbound, MANAGED_MIXED_TAG};
    use serde_json::json;

    #[test]
    fn appends_mixed_when_native_subscription_has_only_tun() {
        let mut content = json!({
            "inbounds": [{
                "tag": "tun-in",
                "listen": { "address": "10.0.0.1", "port": 0 },
                "protocol": { "type": "tun" }
            }]
        });

        ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert_eq!(content["inbounds"].as_array().unwrap().len(), 2);
        assert_eq!(content["inbounds"][1]["tag"], MANAGED_MIXED_TAG);
        assert_eq!(content["inbounds"][1]["listen"]["port"], 7890);
    }

    #[test]
    fn overrides_legacy_managed_port() {
        let mut content = json!({
            "inbounds": [{
                "tag": "mixed-in",
                "listen": { "address": "127.0.0.1", "port": 15581 },
                "protocol": { "type": "mixed" }
            }]
        });

        ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 8899).unwrap();

        assert_eq!(content["inbounds"][0]["listen"]["port"], 8899);
    }

    #[test]
    fn fills_incomplete_local_inbound() {
        let mut content = json!({
            "inbounds": [{
                "tag": "custom-mixed",
                "protocol": { "type": "mixed" }
            }]
        });

        ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert_eq!(content["inbounds"][0]["listen"]["address"], "127.0.0.1");
        assert_eq!(content["inbounds"][0]["listen"]["port"], 7890);
    }

    #[test]
    fn preserves_complete_custom_local_inbound() {
        let mut content = json!({
            "inbounds": [{
                "tag": "custom-mixed",
                "listen": { "address": "127.0.0.1", "port": 9988 },
                "protocol": { "type": "mixed" }
            }]
        });

        ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert_eq!(content["inbounds"][0]["listen"]["port"], 9988);
    }
}
