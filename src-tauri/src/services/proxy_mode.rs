use serde_json::{Map, Value};
use tauri::{AppHandle, State};

use crate::errors::{AppError, AppResult};
use crate::models::{
    core_process::CoreProcessState,
    gui_core::{GuiProxyMode, GuiProxyModeStatus, GuiSetProxyModeInput},
    logs::LogLevel,
};
use crate::services::{
    common, core_config, core_process, logs, proxy_config, system_proxy, system_proxy_guard,
};
use crate::state::app_state::AppState;

const GROUP_KEYS: &[&str] = &[
    "outbound_groups",
    "policy_groups",
    "policies",
    "proxy-groups",
    "proxy_groups",
];
const OUTBOUND_KEYS: &[&str] = &["outbounds", "proxies"];

pub fn status(state: &AppState) -> AppResult<GuiProxyModeStatus> {
    let active = active_proxy_config(state)?;
    let core_running = core_process::refresh_status(state)?.state == CoreProcessState::Running;
    Ok(build_status_from_active(
        active.as_ref(),
        false,
        core_running,
        false,
        false,
        None,
    ))
}

pub async fn set(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: GuiSetProxyModeInput,
) -> AppResult<GuiProxyModeStatus> {
    let _operation = state.proxy_config_operation().lock().await;
    let requested_mode = input.mode;
    let global_outbound = common::normalize_optional(input.global_outbound);
    let restart_core = input.restart_core.unwrap_or(false);

    let core_was_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let managed_proxy_enabled = system_proxy_guard::is_enabled_by_guard().unwrap_or(false);

    // Build the next profile snapshot without mutating shared state. Persisting
    // through the proxy-config transaction keeps the domain file, app config,
    // and in-memory profiles on the same revision.
    let previous_profiles = common::lock(state.proxy_configs(), "proxy_config")?.clone();
    let mut next_profiles = previous_profiles.clone();
    {
        let active = next_profiles
            .iter_mut()
            .find(|profile| profile.active)
            .ok_or_else(|| AppError::invalid_argument("no active proxy config"))?;
        let content = active.content.as_mut().ok_or_else(|| {
            AppError::invalid_argument("active proxy config does not contain JSON content")
        })?;
        if !content.is_object() {
            return Err(AppError::invalid_argument(
                "active proxy config content must be a JSON object",
            ));
        }

        apply_route_mode(content, &requested_mode, global_outbound.as_deref())?;
        active.capabilities = proxy_config::analyze_capabilities(Some(content));
        active.updated_at_unix_ms = common::now_unix_ms();
    }

    proxy_config::persist_profile_transition(state.inner(), &previous_profiles, next_profiles)?;
    if let Err(error) = core_config::export_active(state.clone()) {
        let _ = restore_mode_profiles(state.clone(), &previous_profiles);
        return Err(error);
    }

    let mut restarted_core = false;
    let mut hot_switched = false;

    if core_was_running {
        // Try mode.set hot-switch first (no restart, no connection interruption)
        let common_rules_enabled = common::lock(state.app_config(), "app_config")?
            .routing
            .inject_common_rules;
        if !restart_core && !common_rules_enabled {
            hot_switched =
                try_hot_mode_set(&requested_mode, global_outbound.as_deref(), state.inner()).await;
        }

        // Fallback: restart kernel if hot-switch failed or user explicitly
        // requested restart. `stop`/`start` are blocking operations
        // (`child.wait`, stderr join, kill backoff, port probe), so run them
        // on the blocking pool just like the dedicated restart command does.
        if !hot_switched {
            logs::znet_log(
                Some(state.inner()),
                LogLevel::Info,
                format!(
                    "proxy mode switch: restarting core for {:?}",
                    requested_mode
                ),
            );
            let restart_app = app_handle.clone();
            let restart_result =
                tauri::async_runtime::spawn_blocking(move || core_process::restart(restart_app))
                    .await
                    .map_err(|e| AppError::internal(format!("proxy mode restart task failed: {e}")))
                    .and_then(|result| result);
            match restart_result {
                Ok(started) => {
                    restarted_core = started.state == CoreProcessState::Running;
                }
                Err(error) => {
                    let rollback_error = restore_mode_profiles(state.clone(), &previous_profiles)
                        .err()
                        .map(|rollback| rollback.message);
                    let rollback_app = app_handle.clone();
                    let runtime_rollback_error = tauri::async_runtime::spawn_blocking(move || {
                        core_process::restart(rollback_app)
                    })
                    .await
                    .map_err(|join| join.to_string())
                    .and_then(|result| result.map_err(|rollback| rollback.message))
                    .err();
                    if managed_proxy_enabled
                        && !system_proxy_guard::is_enabled_by_guard().unwrap_or(false)
                    {
                        let endpoint = common::lock(state.app_config(), "app_config")?
                            .local_proxy
                            .clone();
                        let _ = system_proxy_guard::enable_with_guard_and_bypass(
                            &endpoint.host,
                            endpoint.port,
                            &endpoint.bypass,
                        );
                    }
                    let mut message = format!(
                        "failed to restart the kernel after changing proxy mode: {}",
                        error.message
                    );
                    if let Some(rollback_error) = rollback_error {
                        message.push_str(&format!("; profile rollback failed: {rollback_error}"));
                    }
                    if let Some(runtime_rollback_error) = runtime_rollback_error {
                        message.push_str(&format!(
                            "; runtime rollback failed: {runtime_rollback_error}"
                        ));
                    }
                    logs::znet_log(Some(state.inner()), LogLevel::Error, message.clone());
                    return Err(AppError::internal(message));
                }
            }
        }
    }

    let core_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    Ok(build_status_from_active(
        active_proxy_config(state.inner())?.as_ref(),
        true,
        core_running,
        restarted_core,
        hot_switched,
        None,
    ))
}

fn restore_mode_profiles(
    state: State<'_, AppState>,
    previous_profiles: &[crate::models::proxy_config::ProxyConfigProfile],
) -> AppResult<()> {
    let current = common::lock(state.proxy_configs(), "proxy_config")?.clone();
    proxy_config::persist_profile_transition(state.inner(), &current, previous_profiles.to_vec())?;
    core_config::export_active(state)?;
    Ok(())
}

/// Try the kernel's `mode.set` command for hot mode switching.
/// Returns `true` if the command succeeded, `false` otherwise.
/// Does not error — the caller falls back to kernel restart on failure.
///
/// `async` because `set_mode` is an async kernel-adapter call. A previous
/// version used `tauri::async_runtime::block_on` here, which would panic
/// with "Cannot start a runtime from within a runtime" if this code ever
/// runs on a tokio worker (e.g. if an async Tauri command ends up in the
/// call chain).
async fn try_hot_mode_set(mode: &GuiProxyMode, outbound: Option<&str>, state: &AppState) -> bool {
    let mode_str = match mode {
        GuiProxyMode::Global => "global",
        GuiProxyMode::Rule => "rule",
        GuiProxyMode::Direct => "direct",
    };

    let opts = core_config::ipc_options_from_app_config(
        &common::lock(state.app_config(), "app_config")
            .map(|c| c.core.clone())
            .unwrap_or_default(),
    );

    let adapter = crate::kernel::zero::ZeroAdapter::new();
    match crate::kernel::adapter::KernelAdapter::set_mode(
        &adapter,
        mode_str.to_string(),
        outbound.map(String::from),
        opts,
    )
    .await
    {
        Ok(_) => {
            logs::znet_log(
                Some(state),
                LogLevel::Info,
                format!("proxy mode hot-switch succeeded: {mode_str}"),
            );
            eprintln!("[ZNet] mode.set hot-switch succeeded: {mode_str}");
            true
        }
        Err(e) => {
            logs::znet_log(
                Some(state),
                LogLevel::Warn,
                format!("proxy mode hot-switch failed for {mode_str}, falling back to restart"),
            );
            eprintln!("[ZNet] mode.set failed, will restart kernel: {e:?}");
            false
        }
    }
}

fn active_proxy_config(
    state: &AppState,
) -> AppResult<Option<crate::models::proxy_config::ProxyConfigProfile>> {
    Ok(common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .cloned())
}

fn build_status_from_active(
    active: Option<&crate::models::proxy_config::ProxyConfigProfile>,
    exported: bool,
    core_running: bool,
    restarted_core: bool,
    requires_reconnect: bool,
    reason: Option<String>,
) -> GuiProxyModeStatus {
    let system_proxy_enabled = system_proxy::status()
        .map(|status| status.enabled)
        .unwrap_or(false);

    let Some(active) = active else {
        return GuiProxyModeStatus {
            mode: None,
            active_proxy_config_id: None,
            global_outbound: None,
            rule_count: 0,
            has_route: false,
            exported,
            core_running,
            restarted_core,
            system_proxy_enabled,
            requires_reconnect,
            reason: reason.or_else(|| Some("no active proxy config".to_string())),
        };
    };

    let route = active
        .content
        .as_ref()
        .and_then(|content| content.get("route"));
    let detected = active.content.as_ref().and_then(detect_route_mode);
    let mode = detected
        .as_ref()
        .map(|detected| detected.mode.clone())
        .unwrap_or(GuiProxyMode::Rule);
    let mode_reason = detected
        .and_then(|detected| detected.reason)
        .or_else(|| reason.clone());

    GuiProxyModeStatus {
        mode: Some(mode),
        active_proxy_config_id: Some(active.id.clone()),
        global_outbound: active.content.as_ref().and_then(route_global_outbound),
        rule_count: active.content.as_ref().map(rule_count).unwrap_or(0),
        has_route: route.is_some(),
        exported,
        core_running,
        restarted_core,
        system_proxy_enabled,
        requires_reconnect,
        reason: mode_reason,
    }
}

pub(crate) fn apply_route_mode(
    content: &mut Value,
    mode: &GuiProxyMode,
    global_outbound: Option<&str>,
) -> AppResult<()> {
    let outbound = match mode {
        GuiProxyMode::Global | GuiProxyMode::Rule => {
            Some(resolve_global_outbound(content, global_outbound))
        }
        GuiProxyMode::Direct => None,
    };

    let root = content.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("active proxy config content must be a JSON object")
    })?;
    remove_legacy_route_mode(root);

    match mode {
        GuiProxyMode::Global => {
            let outbound = outbound.clone().unwrap_or_else(|| "proxy".to_string());
            set_top_level_mode(
                root,
                serde_json::json!({ "type": "global", "outbound": outbound }),
            );
            ensure_route_final(root, serde_json::json!({ "type": "direct" }));
        }
        GuiProxyMode::Rule => {
            set_top_level_mode(root, serde_json::json!({ "type": "rule" }));
            let outbound = outbound.clone().unwrap_or_else(|| "proxy".to_string());
            ensure_route_final(
                root,
                serde_json::json!({ "type": "route", "outbound": outbound }),
            );
        }
        GuiProxyMode::Direct => {
            set_top_level_mode(root, serde_json::json!({ "type": "direct" }));
            ensure_route_final(root, serde_json::json!({ "type": "direct" }));
        }
    };

    Ok(())
}

fn ensure_object_field<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    let needs_replace = !root.get(key).is_some_and(Value::is_object);
    if needs_replace {
        root.insert(key.to_string(), Value::Object(Map::new()));
    }

    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("route object is inserted before access")
}

fn set_top_level_mode(root: &mut Map<String, Value>, mode: Value) {
    root.insert("mode".to_string(), mode);
}

fn remove_legacy_route_mode(root: &mut Map<String, Value>) {
    if let Some(route) = root.get_mut("route").and_then(Value::as_object_mut) {
        route.remove("mode");
    }
}

fn ensure_route_final(root: &mut Map<String, Value>, default_final: Value) {
    let route = ensure_object_field(root, "route");
    route
        .entry("final".to_string())
        .or_insert_with(|| default_final);
}

#[derive(Clone, Debug)]
pub(crate) struct DetectedRouteMode {
    pub(crate) mode: GuiProxyMode,
    pub(crate) reason: Option<String>,
}

pub(crate) fn detect_route_mode(content: &Value) -> Option<DetectedRouteMode> {
    if let Some(mode_value) = content.get("mode") {
        if let Some(detected) = detect_mode_value(mode_value) {
            return Some(detected);
        }
    }
    if let Some(mode_value) = content.get("route").and_then(|route| route.get("mode")) {
        if let Some(detected) = detect_mode_value(mode_value) {
            return Some(detected);
        }
    }

    let route = content.get("route")?;
    if route
        .get("final")
        .and_then(|final_route| final_route.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("direct"))
    {
        return Some(DetectedRouteMode {
            mode: GuiProxyMode::Direct,
            reason: None,
        });
    }

    Some(DetectedRouteMode {
        mode: GuiProxyMode::Rule,
        reason: None,
    })
}

pub(crate) fn route_global_outbound(content: &Value) -> Option<String> {
    content
        .get("mode")
        .and_then(|mode| mode.get("outbound"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            content
                .get("route")
                .and_then(|route| route.get("mode"))
                .and_then(|mode| mode.get("outbound"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .or_else(|| {
            content
                .get("route")
                .and_then(|route| route.get("final"))
                .and_then(|final_route| final_route.get("outbound"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
}

fn rule_count(content: &Value) -> usize {
    content
        .get("route")
        .and_then(|route| route.get("rules"))
        .and_then(Value::as_array)
        .or_else(|| content.get("rules").and_then(Value::as_array))
        .map(Vec::len)
        .unwrap_or(0)
}

pub(crate) fn resolve_global_outbound(content: &Value, provided: Option<&str>) -> String {
    provided
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| route_global_outbound(content))
        .or_else(|| find_tag(content, GROUP_KEYS, "proxy"))
        .or_else(|| find_tag(content, OUTBOUND_KEYS, "proxy"))
        .or_else(|| find_first_non_direct_tag(content, GROUP_KEYS))
        .or_else(|| find_first_non_direct_tag(content, OUTBOUND_KEYS))
        .or_else(|| find_first_tag(content, GROUP_KEYS))
        .or_else(|| find_first_tag(content, OUTBOUND_KEYS))
        .unwrap_or_else(|| "proxy".to_string())
}

fn find_tag(content: &Value, array_keys: &[&str], expected: &str) -> Option<String> {
    array_keys.iter().find_map(|key| {
        content
            .get(*key)
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find_map(|item| {
                    item_tag(item).filter(|tag| tag.eq_ignore_ascii_case(expected))
                })
            })
    })
}

fn find_first_non_direct_tag(content: &Value, array_keys: &[&str]) -> Option<String> {
    array_keys.iter().find_map(|key| {
        content
            .get(*key)
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find_map(|item| {
                    item_tag(item).filter(|tag| !tag.eq_ignore_ascii_case("direct"))
                })
            })
    })
}

fn find_first_tag(content: &Value, array_keys: &[&str]) -> Option<String> {
    array_keys.iter().find_map(|key| {
        content
            .get(*key)
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find_map(item_tag))
    })
}

fn item_tag(item: &Value) -> Option<String> {
    item.get("tag")
        .or_else(|| item.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToString::to_string)
}

fn detect_mode_value(mode_value: &Value) -> Option<DetectedRouteMode> {
    let raw_mode = mode_value
        .as_str()
        .or_else(|| mode_value.get("type").and_then(Value::as_str))
        .or_else(|| mode_value.get("kind").and_then(Value::as_str))?
        .trim()
        .to_ascii_lowercase();

    Some(match raw_mode.as_str() {
        "global" => DetectedRouteMode {
            mode: GuiProxyMode::Global,
            reason: None,
        },
        "rule" => DetectedRouteMode {
            mode: GuiProxyMode::Rule,
            reason: None,
        },
        "direct" => DetectedRouteMode {
            mode: GuiProxyMode::Direct,
            reason: None,
        },
        _ => DetectedRouteMode {
            mode: GuiProxyMode::Rule,
            reason: Some(format!("unknown route mode `{raw_mode}`, treated as rule")),
        },
    })
}
