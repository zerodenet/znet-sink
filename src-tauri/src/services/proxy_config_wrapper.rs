#[path = "proxy_config.rs"]
mod original;

pub use original::{
    activate_runtime, analyze_capabilities, extract_local_proxy, get, import, import_runtime, list,
    parse_config_content, remove, remove_runtime, set_active, update_active_content, upsert,
    LocalProxyEndpoint,
};
pub(crate) use original::{
    clear_local_proxy_source, ensure_managed_system_proxy_compatible,
    retarget_managed_system_proxy, sync_local_proxy_from_profile, upsert_runtime_locked,
};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppLocalProxyConfig;
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
    is_local_proxy_inbound(inbound)
        && inbound
            .get("tag")
            .and_then(Value::as_str)
            .is_some_and(|tag| matches!(tag.trim(), MANAGED_MIXED_TAG | LEGACY_MANAGED_MIXED_TAG))
}

fn local_inbound_is_usable(inbound: &Value) -> bool {
    original::extract_local_proxy(&json!({ "inbounds": [inbound.clone()] })).is_some()
}

fn resolve_managed_endpoint(config: &AppLocalProxyConfig) -> (String, u16) {
    (config.host.clone(), config.port)
}

#[cfg(test)]
fn has_managed_local_inbound(content: &Value) -> bool {
    content
        .get("inbounds")
        .and_then(Value::as_array)
        .is_some_and(|inbounds| inbounds.iter().any(is_managed_local_inbound))
}

pub(crate) fn project_endpoint(
    content: &mut Value,
    settings: &AppLocalProxyConfig,
    force: bool,
) -> AppResult<()> {
    let root = content
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("config must be an object"))?;
    if !root.contains_key("inbounds") {
        root.insert("inbounds".into(), json!([{
            "tag": MANAGED_MIXED_TAG, "listen": {"address": settings.host, "port": settings.port},
            "protocol": {"type": "mixed"}
        }]));
        return Ok(());
    }
    let inbounds = root
        .get_mut("inbounds")
        .unwrap()
        .as_array_mut()
        .ok_or_else(|| AppError::invalid_argument("inbounds must be an array"))?;
    if let Some(inbound) = inbounds.iter_mut().find(|v| is_local_proxy_inbound(v)) {
        let listen = inbound
            .as_object_mut()
            .unwrap()
            .entry("listen")
            .or_insert_with(|| json!({}));
        let listen = listen
            .as_object_mut()
            .ok_or_else(|| AppError::invalid_argument("listen must be an object"))?;
        for (key, value) in [
            ("address", json!(settings.host)),
            ("port", json!(settings.port)),
        ] {
            if force || !listen.contains_key(key) {
                listen.insert(key.into(), value);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn project_managed_endpoint(
    content: &mut Value,
    settings: &AppLocalProxyConfig,
) -> AppResult<()> {
    project_endpoint(content, settings, true)
}

fn configured_managed_endpoint(state: &AppState) -> AppResult<(String, u16)> {
    let config = lock(state.app_config(), "app_config")?;
    Ok(resolve_managed_endpoint(&config.local_proxy))
}

fn set_managed_endpoint(inbound: &mut Value, host: &str, port: u16) -> AppResult<()> {
    let object = inbound.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("subscription local inbound must be an object")
    })?;
    object
        .entry("tag")
        .or_insert_with(|| json!(MANAGED_MIXED_TAG));
    let listen = object
        .entry("listen".to_string())
        .or_insert_with(|| json!({}));
    let listen = listen.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("subscription local inbound listen must be an object")
    })?;
    listen.entry("address").or_insert_with(|| json!(host));
    listen.entry("port").or_insert_with(|| json!(port));
    Ok(())
}

fn ensure_subscription_local_inbound(
    content: &mut Value,
    existing_content: Option<&Value>,
    host: &str,
    port: u16,
) -> AppResult<bool> {
    let explicit_inbounds = content.get("inbounds").is_some();
    let object = content
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("subscription must produce a JSON object"))?;
    if !object.contains_key("inbounds") {
        object.insert("inbounds".to_string(), Value::Array(Vec::new()));
    }
    let inbounds = object
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::invalid_argument("subscription inbounds must be an array"))?;

    if explicit_inbounds && inbounds.is_empty() {
        return Ok(false);
    }
    let mut incomplete_local_index = None;
    for (index, inbound) in inbounds.iter_mut().enumerate() {
        if !is_local_proxy_inbound(inbound) {
            continue;
        }
        if is_managed_local_inbound(inbound) && !local_inbound_is_usable(inbound) {
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
    use super::{ensure_subscription_local_inbound, resolve_managed_endpoint, MANAGED_MIXED_TAG};
    use crate::models::app_config::AppLocalProxyConfig;
    use serde_json::json;

    #[test]
    fn legacy_source_marker_cannot_replace_the_persisted_client_endpoint() {
        let mut config = AppLocalProxyConfig::default();
        config.port = 15581;
        config.source_proxy_config_id = Some("legacy-profile".to_string());

        assert_eq!(
            resolve_managed_endpoint(&config),
            ("127.0.0.1".to_string(), 15581)
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
    fn effective_endpoint_changes_only_the_client_owned_inbound() {
        let source = json!({"inbounds": [
            {"tag": MANAGED_MIXED_TAG, "protocol":{"type":"mixed"}, "listen":{"address":"127.0.0.1","port":7890}},
            {"tag": "custom-http", "protocol":{"type":"http"}, "listen":{"address":"127.0.0.2","port":8090}}
        ]});
        let mut effective = source.clone();
        let mut settings = AppLocalProxyConfig::default();
        settings.port = 8899;
        super::project_managed_endpoint(&mut effective, &settings).unwrap();
        assert_eq!(effective["inbounds"][0]["listen"]["port"], 8899);
        assert_eq!(effective["inbounds"][1], source["inbounds"][1]);
        assert_eq!(source["inbounds"][0]["listen"]["port"], 7890);
        assert!(super::has_managed_local_inbound(&effective));
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
    fn preserves_source_port_even_with_legacy_managed_tag() {
        let mut content = json!({
            "inbounds": [{
                "tag": "mixed-in",
                "listen": { "address": "127.0.0.1", "port": 15581 },
                "protocol": { "type": "mixed" }
            }]
        });

        let managed =
            ensure_subscription_local_inbound(&mut content, None, "127.0.0.1", 7890).unwrap();

        assert!(!managed);
        assert_eq!(content["inbounds"][0]["tag"], "mixed-in");
        assert_eq!(content["inbounds"][0]["listen"]["port"], 15581);
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
        assert_eq!(content["inbounds"][0]["tag"], "custom-mixed");
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
