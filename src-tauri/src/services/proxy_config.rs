use std::fs;

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::core_process::CoreProcessState;
use crate::models::proxy_config::{
    ProxyConfigCapabilities, ProxyConfigImport, ProxyConfigProfile, ProxyConfigUpsert,
};
use crate::services::common::{
    generated_store_id, lock, normalize_optional, normalize_required, now_unix_ms,
};
use crate::services::domain_store;
use crate::services::{
    app_config, app_config_store, core_config, core_process, system_proxy_guard,
};
use crate::state::app_state::AppState;

fn normalize_config_format(input: Option<String>) -> AppResult<String> {
    let Some(format) = normalize_optional(input) else {
        return Ok("json".to_string());
    };

    if format.trim().eq_ignore_ascii_case("json") {
        Ok("json".to_string())
    } else {
        Err(AppError::invalid_argument(
            "proxy config format must be json",
        ))
    }
}

fn normalize_active_flag(items: &mut [ProxyConfigProfile]) {
    let mut active_index = None;
    for (index, item) in items.iter_mut().enumerate() {
        if item.active {
            if active_index.is_none() {
                active_index = Some(index);
            } else {
                item.active = false;
            }
        }
    }

    if !items.is_empty() && active_index.is_none() {
        items[0].active = true;
    }
}

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
    let previous = lock(state.proxy_configs(), "proxy_config")?.clone();
    let (next, profile) = build_upsert_profiles(&previous, input)?;
    persist_profile_transition(state.inner(), &previous, next)?;
    Ok(profile)
}

fn build_upsert_profiles(
    previous: &[ProxyConfigProfile],
    input: ProxyConfigUpsert,
) -> AppResult<(Vec<ProxyConfigProfile>, ProxyConfigProfile)> {
    let name = normalize_required(input.name, "name")?;
    let id = normalize_optional(input.id).unwrap_or_else(|| generated_store_id("proxy-config"));
    let kernel = normalize_optional(input.kernel).unwrap_or_else(|| "zero".to_string());
    let format = normalize_config_format(input.format)?;
    let path = normalize_optional(input.path);
    let active = input.active.unwrap_or_else(|| {
        previous
            .iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.active)
            .unwrap_or(previous.is_empty())
    });
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

    let mut profiles = previous.to_vec();
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
    normalize_active_flag(&mut profiles);
    let profile = profiles
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| AppError::internal("failed to persist proxy config"))?;
    Ok((profiles, profile))
}

pub(crate) fn persist_profile_transition(
    state: &AppState,
    previous: &[ProxyConfigProfile],
    next: Vec<ProxyConfigProfile>,
) -> AppResult<()> {
    if let Some(active) = next.iter().find(|profile| profile.active) {
        ensure_managed_system_proxy_compatible(active.content.as_ref())?;
    }
    domain_store::save_proxy_configs(&next)?;
    let local_proxy_result = if let Some(active) = next.iter().find(|profile| profile.active) {
        sync_local_proxy_from_profile(state, active)
    } else {
        clear_local_proxy_source(state)
    };
    if let Err(error) = local_proxy_result {
        let _ = domain_store::save_proxy_configs(previous);
        return Err(error);
    }
    *lock(state.proxy_configs(), "proxy_config")? = next;
    Ok(())
}

pub fn import(
    state: State<'_, AppState>,
    input: ProxyConfigImport,
) -> AppResult<ProxyConfigProfile> {
    upsert(state, import_to_upsert(input)?)
}

fn import_to_upsert(input: ProxyConfigImport) -> AppResult<ProxyConfigUpsert> {
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
    Ok(ProxyConfigUpsert {
        id: input.id,
        name: input.name,
        kernel: input.kernel,
        format: Some(normalize_config_format(input.format)?),
        path: input.path,
        content: Some(parsed),
        active: input.active,
    })
}

pub async fn upsert_runtime(
    app_handle: AppHandle,
    input: ProxyConfigUpsert,
) -> AppResult<ProxyConfigProfile> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let previous = lock(state.proxy_configs(), "proxy_config")?.clone();
    let previous_active = previous.iter().find(|profile| profile.active).cloned();
    let (next, profile) = build_upsert_profiles(&previous, input)?;
    let next_active = next.iter().find(|item| item.active).cloned();
    let runtime_changed = previous_active
        .as_ref()
        .map(|item| (&item.id, &item.content))
        != next_active.as_ref().map(|item| (&item.id, &item.content));
    let running = core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;

    if !running || !runtime_changed {
        persist_profile_transition(state.inner(), &previous, next)?;
        retarget_managed_system_proxy(state.inner())?;
        return Ok(profile);
    }

    let next_active = next_active.ok_or_else(|| {
        AppError::invalid_argument("a running kernel requires an active proxy config")
    })?;
    let content = next_active.content.clone().ok_or_else(|| {
        AppError::invalid_argument("cannot apply a proxy config without parsed content")
    })?;
    let content = crate::services::rule_overlay::compose_effective_config(state.inner(), &content)?;
    match ZeroAdapter::new()
        .apply_config(content, ipc_options(state.inner())?)
        .await
    {
        Ok(_) => {
            if let Err(error) = persist_profile_transition(state.inner(), &previous, next) {
                if let Some(previous_active) = previous_active.as_ref() {
                    let _ = reapply_profile(state.inner(), previous_active).await;
                }
                return Err(error);
            }
            if let Err(error) = retarget_managed_system_proxy(state.inner()) {
                let _ = restore_profiles(state.inner(), &previous);
                if let Some(previous_active) = previous_active.as_ref() {
                    let _ = reapply_profile(state.inner(), previous_active).await;
                }
                let _ = retarget_managed_system_proxy(state.inner());
                return Err(error);
            }
            Ok(profile)
        }
        Err(_) => {
            let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
            persist_profile_transition(state.inner(), &previous, next)?;
            if let Err(error) = restart_core(app_handle.clone()).await {
                let _ = restore_profiles(state.inner(), &previous);
                let _ = restart_core(app_handle.clone()).await;
                let _ =
                    restore_managed_system_proxy_if_needed(state.inner(), managed_proxy_enabled);
                return Err(AppError::internal(format!(
                    "failed to restart the kernel after saving proxy config: {}",
                    error.message
                )));
            }
            Ok(profile)
        }
    }
}

pub async fn import_runtime(
    app_handle: AppHandle,
    input: ProxyConfigImport,
) -> AppResult<ProxyConfigProfile> {
    upsert_runtime(app_handle, import_to_upsert(input)?).await
}

pub fn set_active(state: State<'_, AppState>, id: String) -> AppResult<ProxyConfigProfile> {
    let id = normalize_required(id, "id")?;
    let previous = lock(state.proxy_configs(), "proxy_config")?.clone();
    if !previous.iter().any(|profile| profile.id == id) {
        return Err(AppError::not_found("proxy_config", id));
    }
    let mut next = previous.clone();
    for profile in next.iter_mut() {
        profile.active = profile.id == id;
    }
    normalize_active_flag(&mut next);
    let active = next
        .iter()
        .find(|profile| profile.active)
        .cloned()
        .ok_or_else(|| AppError::internal("failed to activate proxy config"))?;
    persist_profile_transition(state.inner(), &previous, next)?;

    Ok(active)
}

/// Mirror a hot-applied config into the active profile's `content`.
///
/// Called after the kernel accepts `config.apply` so that config-derived
/// views (proxy nodes, policy groups) and the next core-process start —
/// which exports `content` to disk via `core_config::export_active` — both
/// reflect the live configuration. Re-derives capabilities and bumps
/// `updated_at`. No-op when no profile is active.
pub fn update_active_content(state: &AppState, content: Value) -> AppResult<()> {
    let previous = lock(state.proxy_configs(), "proxy_config")?.clone();
    let mut next = previous.clone();
    let Some(active) = next.iter_mut().find(|profile| profile.active) else {
        return Ok(());
    };
    active.capabilities = analyze_capabilities(Some(&content));
    active.content = Some(content);
    active.updated_at_unix_ms = now_unix_ms();
    persist_profile_transition(state, &previous, next)?;
    Ok(())
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let id = normalize_required(id, "id")?;
    let previous = lock(state.proxy_configs(), "proxy_config")?.clone();
    let mut next = previous.clone();
    next.retain(|profile| profile.id != id);
    if next.len() == previous.len() {
        return Err(AppError::not_found("proxy_config", id));
    }
    normalize_active_flag(&mut next);
    persist_profile_transition(state.inner(), &previous, next)?;

    Ok(())
}

pub async fn activate_runtime(app_handle: AppHandle, id: String) -> AppResult<ProxyConfigProfile> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let id = normalize_required(id, "id")?;
    let (previous_active, target) = {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let target = profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| AppError::not_found("proxy_config", id.clone()))?;
        let previous = profiles.iter().find(|profile| profile.active).cloned();
        (previous, target)
    };
    if target.active {
        return Ok(target);
    }

    let running = core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    if !running {
        let active = set_active(state.clone(), id)?;
        retarget_managed_system_proxy(state.inner())?;
        return Ok(active);
    }

    let content = target.content.clone().ok_or_else(|| {
        AppError::invalid_argument("cannot activate a proxy config without parsed content")
    })?;
    let content = crate::services::rule_overlay::compose_effective_config(state.inner(), &content)?;
    let options = ipc_options(state.inner())?;
    match ZeroAdapter::new().apply_config(content, options).await {
        Ok(_) => match set_active(state.clone(), id) {
            Ok(active) => {
                if let Err(error) = retarget_managed_system_proxy(state.inner()) {
                    rollback_hot_activation(state.clone(), previous_active.as_ref()).await;
                    return Err(error);
                }
                Ok(active)
            }
            Err(error) => {
                rollback_hot_activation(state.clone(), previous_active.as_ref()).await;
                Err(error)
            }
        },
        Err(_) => {
            let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
            let active = set_active(state.clone(), id)?;
            if let Err(error) = restart_core(app_handle.clone()).await {
                rollback_restarted_activation(
                    app_handle.clone(),
                    state.clone(),
                    previous_active.as_ref(),
                    managed_proxy_enabled,
                )
                .await;
                return Err(AppError::internal(format!(
                    "failed to restart the kernel with proxy config '{}': {}",
                    active.name, error.message
                )));
            }
            Ok(active)
        }
    }
}

pub async fn remove_runtime(app_handle: AppHandle, id: String) -> AppResult<()> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let id = normalize_required(id, "id")?;
    let (removed, replacement) = {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let removed = profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| AppError::not_found("proxy_config", id.clone()))?;
        let replacement = removed
            .active
            .then(|| profiles.iter().find(|profile| profile.id != id).cloned())
            .flatten();
        (removed, replacement)
    };
    if !removed.active {
        return remove(state.clone(), id);
    }

    let running = core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);
    let Some(replacement) = replacement else {
        if running {
            stop_core(app_handle.clone()).await?;
        } else if managed_proxy_enabled {
            system_proxy_guard::disable_with_guard()?;
        }
        if let Err(error) = remove(state.clone(), id) {
            if running {
                let _ = restart_core(app_handle.clone()).await;
            }
            let _ = restore_managed_system_proxy_if_needed(state.inner(), managed_proxy_enabled);
            return Err(error);
        }
        return Ok(());
    };

    if !running {
        remove(state.clone(), id)?;
        retarget_managed_system_proxy(state.inner())?;
        return Ok(());
    }

    let content = replacement.content.clone().ok_or_else(|| {
        AppError::invalid_argument("cannot promote a proxy config without parsed content")
    })?;
    let content = crate::services::rule_overlay::compose_effective_config(state.inner(), &content)?;
    let options = ipc_options(state.inner())?;
    match ZeroAdapter::new().apply_config(content, options).await {
        Ok(_) => {
            if let Err(error) = remove(state.clone(), id) {
                let _ = reapply_profile(state.inner(), &removed).await;
                return Err(error);
            }
            if let Err(error) = retarget_managed_system_proxy(state.inner()) {
                let _ = restore_removed_profile(state.clone(), removed.clone());
                let _ = reapply_profile(state.inner(), &removed).await;
                let _ = retarget_managed_system_proxy(state.inner());
                return Err(error);
            }
            Ok(())
        }
        Err(_) => {
            remove(state.clone(), id)?;
            if let Err(error) = restart_core(app_handle.clone()).await {
                let _ = restore_removed_profile(state.clone(), removed.clone());
                let _ = restart_core(app_handle.clone()).await;
                let _ =
                    restore_managed_system_proxy_if_needed(state.inner(), managed_proxy_enabled);
                return Err(AppError::internal(format!(
                    "failed to restart the kernel after deleting proxy config: {}",
                    error.message
                )));
            }
            Ok(())
        }
    }
}

fn ipc_options(state: &AppState) -> AppResult<crate::models::core::CoreIpcOptions> {
    let config = lock(state.app_config(), "app_config")?;
    Ok(core_config::ipc_options_from_app_config(&config.core))
}

async fn reapply_profile(state: &AppState, profile: &ProxyConfigProfile) -> AppResult<()> {
    let Some(content) = profile.content.clone() else {
        return Ok(());
    };
    let content = crate::services::rule_overlay::compose_effective_config(state, &content)?;
    ZeroAdapter::new()
        .apply_config(content, ipc_options(state)?)
        .await
        .map(|_| ())
}

async fn rollback_hot_activation(
    state: State<'_, AppState>,
    previous: Option<&ProxyConfigProfile>,
) {
    let Some(previous) = previous else {
        return;
    };
    let _ = reapply_profile(state.inner(), previous).await;
    let _ = set_active(state.clone(), previous.id.clone());
    let _ = retarget_managed_system_proxy(state.inner());
}

async fn rollback_restarted_activation(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    previous: Option<&ProxyConfigProfile>,
    managed_proxy_enabled: bool,
) {
    let Some(previous) = previous else {
        return;
    };
    if set_active(state.clone(), previous.id.clone()).is_ok() {
        let _ = restart_core(app_handle).await;
        let _ = restore_managed_system_proxy_if_needed(state.inner(), managed_proxy_enabled);
    }
}

fn restore_removed_profile(
    state: State<'_, AppState>,
    removed: ProxyConfigProfile,
) -> AppResult<()> {
    let previous = lock(state.proxy_configs(), "proxy_config")?.clone();
    let mut next = previous.clone();
    if removed.active {
        for profile in &mut next {
            profile.active = false;
        }
    }
    next.push(removed.clone());
    normalize_active_flag(&mut next);
    persist_profile_transition(state.inner(), &previous, next)?;
    Ok(())
}

fn restore_profiles(state: &AppState, previous: &[ProxyConfigProfile]) -> AppResult<()> {
    let current = lock(state.proxy_configs(), "proxy_config")?.clone();
    persist_profile_transition(state, &current, previous.to_vec())
}

async fn restart_core(app_handle: AppHandle) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || core_process::restart(app_handle).map(|_| ()))
        .await
        .map_err(|error| AppError::internal(format!("core restart task failed: {error}")))?
}

async fn stop_core(app_handle: AppHandle) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        core_process::stop(state).map(|_| ())
    })
    .await
    .map_err(|error| AppError::internal(format!("core stop task failed: {error}")))?
}

fn local_proxy_endpoint(state: &AppState) -> AppResult<(String, u16, Vec<String>)> {
    let config = lock(state.app_config(), "app_config")?;
    Ok((
        config.local_proxy.host.clone(),
        config.local_proxy.port,
        config.local_proxy.bypass.clone(),
    ))
}

pub(crate) fn retarget_managed_system_proxy(state: &AppState) -> AppResult<()> {
    let (host, port, _) = local_proxy_endpoint(state)?;
    system_proxy_guard::retarget_if_enabled(&host, port)
}

fn enable_managed_system_proxy(state: &AppState) -> AppResult<()> {
    let (host, port, bypass) = local_proxy_endpoint(state)?;
    system_proxy_guard::enable_with_guard_and_bypass(&host, port, &bypass)
}

fn restore_managed_system_proxy_if_needed(state: &AppState, was_enabled: bool) -> AppResult<()> {
    if was_enabled && !system_proxy_guard::is_enabled_by_guard().unwrap_or(false) {
        enable_managed_system_proxy(state)?;
    }
    Ok(())
}

fn clear_local_proxy_source(state: &AppState) -> AppResult<()> {
    let mut next = lock(state.app_config(), "app_config")?.clone();
    next.local_proxy.source_proxy_config_id = None;
    app_config_store::save(&app_config_store::default_config_path()?, &next)?;
    *lock(state.app_config(), "app_config")? = next;
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
    .filter(|(_, enabled)| *enabled)
    .map(|(key, _)| key.to_string())
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
        .and_then(|inbounds| inbounds.iter().find_map(extract_inbound_endpoint))
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
    let protocol = inbound
        .get("protocol")?
        .get("type")?
        .as_str()?
        .trim()
        .to_ascii_lowercase();
    if !matches!(protocol.as_str(), "mixed" | "http" | "socks5") {
        return None;
    }
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
        host: match host {
            "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
            _ => host.to_string(),
        },
        port,
    })
}

pub(crate) fn sync_local_proxy_from_profile(
    state: &AppState,
    profile: &ProxyConfigProfile,
) -> AppResult<()> {
    let mut next = lock(state.app_config(), "app_config")?.clone();
    if let Some(endpoint) = profile.content.as_ref().and_then(extract_local_proxy) {
        app_config::validate_port(endpoint.port, "localProxy.port")?;
        next.local_proxy.host = endpoint.host;
        next.local_proxy.port = endpoint.port;
        next.local_proxy.source_proxy_config_id = Some(profile.id.clone());
    } else {
        next.local_proxy.source_proxy_config_id = None;
    }
    app_config_store::save(&app_config_store::default_config_path()?, &next)?;
    *lock(state.app_config(), "app_config")? = next;
    Ok(())
}

fn ensure_managed_system_proxy_compatible(content: Option<&Value>) -> AppResult<()> {
    if system_proxy_guard::is_enabled_by_guard()? && content.and_then(extract_local_proxy).is_none()
    {
        return Err(AppError::invalid_argument(
            "disable the GUI-managed system proxy before activating a config without a local proxy inbound",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        build_upsert_profiles, extract_local_proxy, ProxyConfigProfile, ProxyConfigUpsert,
    };

    fn profile(id: &str, active: bool) -> ProxyConfigProfile {
        ProxyConfigProfile {
            id: id.to_string(),
            name: id.to_string(),
            kernel: "zero".to_string(),
            format: "json".to_string(),
            path: None,
            content: Some(json!({ "id": id })),
            active,
            updated_at_unix_ms: 1,
            capabilities: Default::default(),
        }
    }

    fn upsert(id: &str, active: Option<bool>) -> ProxyConfigUpsert {
        ProxyConfigUpsert {
            id: Some(id.to_string()),
            name: format!("{id}-updated"),
            kernel: None,
            format: None,
            path: None,
            content: Some(json!({ "updated": true })),
            active,
        }
    }

    #[test]
    fn editing_active_profile_without_flag_preserves_activation() {
        let previous = vec![profile("inactive", false), profile("active", true)];
        let (next, updated) = build_upsert_profiles(&previous, upsert("active", None)).unwrap();

        assert!(updated.active);
        assert_eq!(next.iter().filter(|profile| profile.active).count(), 1);
        assert!(
            next.iter()
                .find(|profile| profile.id == "active")
                .unwrap()
                .active
        );
    }

    #[test]
    fn activating_profile_deactivates_every_other_profile() {
        let previous = vec![profile("first", true), profile("second", false)];
        let (next, updated) =
            build_upsert_profiles(&previous, upsert("second", Some(true))).unwrap();

        assert!(updated.active);
        assert!(
            !next
                .iter()
                .find(|profile| profile.id == "first")
                .unwrap()
                .active
        );
        assert_eq!(next.iter().filter(|profile| profile.active).count(), 1);
    }

    #[test]
    fn local_proxy_ignores_server_inbounds_and_normalizes_wildcard_listen() {
        let config = json!({
            "inbounds": [
                {
                    "tag": "server",
                    "listen": { "address": "0.0.0.0", "port": 443 },
                    "protocol": { "type": "trojan", "password": "secret" }
                },
                {
                    "tag": "mixed-in",
                    "listen": { "address": "0.0.0.0", "port": 7890 },
                    "protocol": { "type": "mixed" }
                }
            ]
        });

        let endpoint = extract_local_proxy(&config).unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 7890);
    }

    #[test]
    fn server_only_config_has_no_system_proxy_endpoint() {
        let config = json!({
            "inbounds": [{
                "tag": "server",
                "listen": { "address": "0.0.0.0", "port": 443 },
                "protocol": { "type": "vless", "users": [] }
            }]
        });

        assert_eq!(extract_local_proxy(&config), None);
    }
}
