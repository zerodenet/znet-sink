use serde_json::{Map, Value};
use tauri::{AppHandle, State};

use crate::errors::{AppError, AppResult};
use crate::models::{
    core_process::CoreProcessState,
    gui_core::{GuiProxyMode, GuiProxyModeStatus, GuiSetProxyModeInput},
};
use crate::services::{common, core_process, proxy_config, system_proxy};
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
    let core_was_running =
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    let previous = active_proxy_config(state.inner())?
        .ok_or_else(|| AppError::invalid_argument("no active proxy config"))?;
    let mut candidate = previous.clone();
    let content = candidate.content.as_mut().ok_or_else(|| {
        AppError::invalid_argument("active proxy config does not contain JSON content")
    })?;
    apply_route_mode(
        content,
        &input.mode,
        common::normalize_optional(input.global_outbound).as_deref(),
    )?;
    // The same confirmed apply-and-persist path owns profile edits, subscription
    // updates and mode changes. A rejected/timed-out apply never implies restart.
    proxy_config::upsert_runtime_locked(app_handle.clone(), profile_input(candidate)).await?;
    let restart = core_was_running && input.restart_core.unwrap_or(false);
    if restart {
        let result = crate::commands::core_process::restart_managed(app_handle.clone())
            .await
            .and_then(|result| result.require_restored());
        if let Err(mut error) = result {
            match proxy_config::upsert_runtime_locked(app_handle.clone(), profile_input(previous))
                .await
            {
                Err(rollback) => error
                    .message
                    .push_str(&format!("; mode rollback failed: {}", rollback.message)),
                Ok(_) => {
                    if let Err(rollback) =
                        crate::commands::core_process::restart_managed(app_handle.clone())
                            .await
                            .and_then(|result| result.require_restored())
                    {
                        error
                            .message
                            .push_str(&format!("; runtime rollback failed: {}", rollback.message));
                    }
                }
            }
            return Err(error);
        }
    }
    Ok(build_status_from_active(
        active_proxy_config(state.inner())?.as_ref(),
        true,
        core_process::refresh_status(state.inner())?.state == CoreProcessState::Running,
        restart,
        core_was_running && !restart,
        None,
    ))
}

fn profile_input(
    profile: crate::models::proxy_config::ProxyConfigProfile,
) -> crate::models::proxy_config::ProxyConfigUpsert {
    crate::models::proxy_config::ProxyConfigUpsert {
        id: Some(profile.id),
        name: profile.name,
        kernel: Some(profile.kernel),
        format: Some(profile.format),
        path: profile.path,
        content: profile.content,
        active: Some(profile.active),
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
