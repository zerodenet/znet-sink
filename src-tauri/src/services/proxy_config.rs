use std::fs;

use serde_json::Value;
use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::proxy_config::{
    ProxyConfigCapabilities, ProxyConfigImport, ProxyConfigProfile, ProxyConfigUpsert,
};
use crate::services::common::{
    generated_id, lock, normalize_optional, normalize_required, now_unix_ms,
};
use crate::services::domain_store;
use crate::services::{app_config, app_config_store};
use crate::state::app_state::AppState;

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<ProxyConfigProfile>> {
    Ok(lock(state.proxy_configs(), "proxy_config")?.clone())
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<ProxyConfigProfile> {
    let id = normalize_required(id, "id")?;
    lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.id == id)
        .cloned()
        .ok_or_else(|| AppError::not_found("proxy_config", id))
}

pub fn upsert(
    state: State<'_, AppState>,
    input: ProxyConfigUpsert,
) -> AppResult<ProxyConfigProfile> {
    let name = normalize_required(input.name, "name")?;
    let id = normalize_optional(input.id)
        .unwrap_or_else(|| generated_id("proxy-config", state.next_record_id()));
    let kernel = normalize_optional(input.kernel).unwrap_or_else(|| "zero".to_string());
    let format = normalize_optional(input.format).unwrap_or_else(|| "zero".to_string());
    let path = normalize_optional(input.path);
    let active = input.active.unwrap_or(false);
    let updated_at_unix_ms = now_unix_ms();
    let capabilities = analyze_capabilities(input.content.as_ref());

    let profile = ProxyConfigProfile {
        id: id.clone(),
        name,
        kernel,
        format,
        path,
        content: input.content,
        active,
        updated_at_unix_ms,
        capabilities,
    };

    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    if profile.active {
        for item in profiles.iter_mut() {
            item.active = false;
        }
    }

    match profiles.iter_mut().find(|item| item.id == id) {
        Some(existing) => {
            *existing = profile.clone();
        }
        None => profiles.push(profile.clone()),
    }
    domain_store::save_proxy_configs(&profiles)?;
    if profile.active {
        sync_local_proxy_from_profile(state.inner(), &profile)?;
    }

    Ok(profile)
}

pub fn import(
    state: State<'_, AppState>,
    input: ProxyConfigImport,
) -> AppResult<ProxyConfigProfile> {
    let content = match (input.content, normalize_optional(input.path.clone())) {
        (Some(content), _) => content,
        (None, Some(path)) => fs::read_to_string(&path).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to read proxy config: {error}"),
            details: Some(serde_json::json!({ "path": path })),
        })?,
        (None, None) => {
            return Err(AppError::invalid_argument(
                "content or path is required to import proxy config",
            ));
        }
    };

    let parsed = parse_config_content(&content)?;
    upsert(
        state,
        ProxyConfigUpsert {
            id: input.id,
            name: input.name,
            kernel: input.kernel,
            format: input.format.or_else(|| Some("json".to_string())),
            path: input.path,
            content: Some(parsed),
            active: input.active,
        },
    )
}

pub fn set_active(state: State<'_, AppState>, id: String) -> AppResult<ProxyConfigProfile> {
    let id = normalize_required(id, "id")?;
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;

    if !profiles.iter().any(|profile| profile.id == id) {
        return Err(AppError::not_found("proxy_config", id));
    }

    let mut active = None;
    for profile in profiles.iter_mut() {
        profile.active = profile.id == id;
        if profile.active {
            active = Some(profile.clone());
        }
    }
    domain_store::save_proxy_configs(&profiles)?;
    if let Some(profile) = active.as_ref() {
        sync_local_proxy_from_profile(state.inner(), profile)?;
    }

    active.ok_or_else(|| AppError::internal("failed to activate proxy config"))
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let before = profiles.len();
    profiles.retain(|profile| profile.id != id);

    if profiles.len() == before {
        return Err(AppError::not_found("proxy_config", id));
    }
    domain_store::save_proxy_configs(&profiles)?;

    Ok(())
}

pub fn analyze_capabilities(config: Option<&Value>) -> ProxyConfigCapabilities {
    let mut capabilities = ProxyConfigCapabilities::default();
    let Some(config) = config else {
        return capabilities;
    };

    capabilities.has_proxy_nodes = has_non_empty_array(config, &["proxies", "outbounds"]);
    capabilities.has_proxy_groups = has_non_empty_array(
        config,
        &["proxy-groups", "proxy_groups", "policy_groups", "policies"],
    );
    capabilities.has_route_rules =
        has_non_empty_nested_array(config, &[&["rules"], &["route", "rules"]]);
    capabilities.has_rule_sets = has_non_empty_array(
        config,
        &["rule-providers", "rule_providers", "rule_sets", "ruleSets"],
    );
    capabilities.has_selector = contains_kind(config, &["select", "selector"]);
    capabilities.has_url_test = contains_kind(config, &["url-test", "urltest", "url_test"]);

    capabilities.feature_keys = [
        ("proxyNodes", capabilities.has_proxy_nodes),
        ("proxyGroups", capabilities.has_proxy_groups),
        ("routing", capabilities.has_route_rules),
        ("ruleSets", capabilities.has_rule_sets),
        ("selector", capabilities.has_selector),
        ("urlTest", capabilities.has_url_test),
    ]
    .into_iter()
    .filter_map(|(key, enabled)| enabled.then(|| key.to_string()))
    .collect();

    capabilities
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalProxyEndpoint {
    pub host: String,
    pub port: u16,
}

pub fn extract_local_proxy(config: &Value) -> Option<LocalProxyEndpoint> {
    config
        .get("inbounds")
        .and_then(Value::as_array)
        .and_then(|inbounds| {
            inbounds
                .iter()
                .find_map(extract_inbound_endpoint)
                .or_else(|| inbounds.first().and_then(extract_inbound_endpoint))
        })
}

pub fn parse_config_content(content: &str) -> AppResult<Value> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::invalid_argument(
            "proxy config content must not be empty",
        ));
    }

    serde_json::from_str(content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("proxy config must be valid JSON: {error}"),
        details: None,
    })
}

fn has_non_empty_array(config: &Value, keys: &[&str]) -> bool {
    keys.iter().any(|key| {
        config
            .get(*key)
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    })
}

fn extract_inbound_endpoint(inbound: &Value) -> Option<LocalProxyEndpoint> {
    let listen = inbound.get("listen")?;
    let port = listen.get("port")?.as_u64()?;
    let port = u16::try_from(port).ok().filter(|port| *port != 0)?;
    let host = listen
        .get("address")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1")
        .trim();
    if host.is_empty() {
        return None;
    }

    Some(LocalProxyEndpoint {
        host: host.to_string(),
        port,
    })
}

pub(crate) fn sync_local_proxy_from_profile(
    state: &AppState,
    profile: &ProxyConfigProfile,
) -> AppResult<()> {
    let Some(content) = profile.content.as_ref() else {
        return Ok(());
    };
    let Some(endpoint) = extract_local_proxy(content) else {
        return Ok(());
    };
    app_config::validate_port(endpoint.port, "localProxy.port")?;

    let mut config = lock(state.app_config(), "app_config")?;
    config.local_proxy.host = endpoint.host;
    config.local_proxy.port = endpoint.port;
    config.local_proxy.source_proxy_config_id = Some(profile.id.clone());
    app_config_store::save(&app_config_store::default_config_path()?, &config)
}

fn has_non_empty_nested_array(config: &Value, paths: &[&[&str]]) -> bool {
    paths.iter().any(|path| {
        path.iter()
            .try_fold(config, |value, key| value.get(*key))
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
    })
}

fn contains_kind(value: &Value, candidates: &[&str]) -> bool {
    match value {
        Value::Object(object) => {
            if let Some(kind) = object
                .get("type")
                .or_else(|| object.get("kind"))
                .and_then(Value::as_str)
            {
                if candidates
                    .iter()
                    .any(|candidate| kind.eq_ignore_ascii_case(candidate))
                {
                    return true;
                }
            }

            object
                .values()
                .any(|value| contains_kind(value, candidates))
        }
        Value::Array(items) => items.iter().any(|value| contains_kind(value, candidates)),
        _ => false,
    }
}
