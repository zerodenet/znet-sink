#[path = "proxy_config.rs"]
mod original;

pub use original::{
    activate_runtime, analyze_capabilities, extract_local_proxy, get, import, import_runtime, list,
    parse_config_content, remove, remove_runtime, set_active, update_active_content, upsert,
    LocalProxyEndpoint,
};
pub(crate) use original::{persist_profile_transition, retarget_managed_system_proxy};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppLocalProxyConfig;
use crate::models::proxy_config::{ProxyConfigProfile, ProxyConfigUpsert};
use crate::services::common::lock;
use crate::state::app_state::AppState;

const MANAGED_MIXED_TAG: &str = "znet-sink-mixed-in";
const LEGACY_MANAGED_MIXED_TAG: &str = "mixed-in";
const DEFAULT_MANAGED_MIXED_HOST: &str = "127.0.0.1";
const DEFAULT_MANAGED_MIXED_PORT: u16 = 7890;

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

fn resolve_managed_endpoint(config: &AppLocalProxyConfig) -> (String, u16) {
    if config.source_proxy_config_id.is_some() {
        return (
            DEFAULT_MANAGED_MIXED_HOST.to_string(),
            DEFAULT_MANAGED_MIXED_PORT,
        );
    }
    (config.host.clone(), config.port)
}

fn configured_managed_endpoint(state: &AppState) -> AppResult<(String, u16)> {
    let config = lock(state.app_config(), "app_config")?;
    Ok(resolve_managed_endpoint(&config.local_proxy))
}

fn set_managed_endpoint(inbound: &mut Value, host: &str, port: u16) -> AppResult<()> {
    let object = inbound.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("subscription local inbound must be an object")
    })?;
    object.insert(
        "tag".to_string(),
        Value::String(MANAGED_MIXED_TAG.to_string()),
    );
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
) -> AppResult<bool> {
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
            return Ok(true);
        }
        if local_inbound_is_usable(inbound) {
            return Ok(false);
        }
        incomplete_local_index.get_or_insert(index);
    }

    if let Some(index) = incomplete_local_index {
        set_managed_endpoint(&mut inbounds[index], host, port)?;
        return Ok(true);
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
        let managed = is_managed_local_inbound(&inbound) || !local_inbound_is_usable(&inbound);
        if managed {
            set_managed_endpoint(&mut inbound, host, port)?;
        }
        inbounds.push(inbound);
        return Ok(managed);
    }

    inbounds.push(json!({
        "tag": MANAGED_MIXED_TAG,
        "listen": { "address": host, "port": port },
        "protocol": { "type": "mixed" }
    }));
    Ok(true)
}

fn prepare_subscription_upsert(state: &AppState, input: &mut ProxyConfigUpsert) -> AppResult<bool> {
    if !is_subscription_source(input) {
        return Ok(false);
    }

    let (host, port) = configured_managed_endpoint(state)?;
    let existing_content = if let Some(id) = input.id.as_ref() {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        profiles
            .iter()
            .find(|profile| profile.id == *id)
            .and_then(|profile| profile.content.clone())
    } else {
        None
    };

    match input.content.as_mut() {
        Some(content) => {
            ensure_subscription_local_inbound(content, existing_content.as_ref(), &host, port)
        }
        None => Ok(false),
    }
}

fn clear_managed_source(state: &AppState) -> AppResult<()> {
    let mut next = lock(state.app_config(), "app_config")?.clone();
    if next.local_proxy.source_proxy_config_id.is_none() {
        return Ok(());
    }
    next.local_proxy.source_proxy_config_id = None;
    crate::services::app_config::replace(state, next)
}

pub async fn upsert_runtime(
    app_handle: AppHandle,
    mut input: ProxyConfigUpsert,
) -> AppResult<ProxyConfigProfile> {
    let managed_by_gui = {
        let state = app_handle.state::<AppState>();
        prepare_subscription_upsert(state.inner(), &mut input)?
    };
    let profile = original::upsert_runtime(app_handle.clone(), input).await?;
    if managed_by_gui {
        let state = app_handle.state::<AppState>();
        clear_managed_source(state.inner())?;
    }
    Ok(profile)
}

#[cfg(test)]
mod wrapper_tests {
    use super::{
        ensure_subscription_local_inbound, resolve_managed_endpoint, MANAGED_MIXED_TAG,
    };
    use crate::models::app_config::AppLocalProxyConfig;
    use serde_json::json;

    #[test]
    fn derived_runtime_endpoint_falls_back_to_7890() {
        let mut config = AppLocalProxyConfig::default();
        config.port = 15581;
        config.source_proxy_config_id = Some("legacy-profile".to_string());

        assert_eq!(
            resolve_managed_endpoint(&config),
            ("127.0.0.1".to_string(), 7890)
        );
    }

    #[test]
    fn explicit_endpoint_preserves_user_choice() {
        let mut config = AppLocalProxyConfig::default();
        config.host = "127.0.0.2".to_string();
        config.port = 8899;
        config.source_proxy_config_id = None;

        assert_eq!(
            resolve_managed_endpoint(&config),
            ("127.0.0.2".to_string(), 8899)
        );
    }

    #[test]
    fn appends_mixed_when_inbounds_are_missing() {
        let mut content = json!({ "outbounds": [] });

        let managed =
            ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert!(managed);
        assert_eq!(content["inbounds"].as_array().unwrap().len(), 1);
        assert_eq!(content["inbounds"][0]["tag"], MANAGED_MIXED_TAG);
        assert_eq!(content["inbounds"][0]["listen"]["address"], "127.0.0.1");
        assert_eq!(content["inbounds"][0]["listen"]["port"], 7890);
    }

    #[test]
    fn appends_mixed_when_native_subscription_has_only_tun() {
        let mut content = json!({
            "inbounds": [{
                "tag": "tun-in",
                "listen": { "address": "10.0.0.1", "port": 0 },
                "protocol": { "type": "tun" }
            }]
        });

        let managed =
            ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert!(managed);
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

        let managed =
            ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert!(managed);
        assert_eq!(content["inbounds"][0]["tag"], MANAGED_MIXED_TAG);
        assert_eq!(content["inbounds"][0]["listen"]["port"], 7890);
    }

    #[test]
    fn fills_incomplete_local_inbound() {
        let mut content = json!({
            "inbounds": [{
                "tag": "custom-mixed",
                "protocol": { "type": "mixed" }
            }]
        });

        let managed =
            ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert!(managed);
        assert_eq!(content["inbounds"][0]["tag"], MANAGED_MIXED_TAG);
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

        let managed =
            ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert!(!managed);
        assert_eq!(content["inbounds"][0]["listen"]["port"], 9988);
    }
}
