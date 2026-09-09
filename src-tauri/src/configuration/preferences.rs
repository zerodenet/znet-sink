//! Resolve client defaults without mutating a source profile or persisted defaults.
use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppTunConfig};
use crate::services::{bypass, common::lock, proxy_config};
use crate::state::app_state::AppState;
use serde_json::{json, Value};

pub(crate) fn source(state: &AppState) -> AppResult<Value> {
    Ok(current(state)?.1)
}

pub(crate) fn current(state: &AppState) -> AppResult<(AppConfig, Value)> {
    let app = lock(state.app_config(), "app_config")?.clone();
    let profile = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .cloned();
    let base = profile
        .as_ref()
        .and_then(|p| p.content.clone())
        .unwrap_or_else(|| json!({}));
    super::local_edits::resolve(&app, profile.as_ref().map(|p| p.id.as_str()), &base)
}

pub(crate) fn owns_tun(state: &AppState) -> AppResult<bool> {
    let (app, _) = current(state)?;
    Ok(!app.overrides.tun
        && source(state)?
            .pointer("/runtime/tun")
            .is_some_and(Value::is_object))
}

/// A hot config apply cannot silently retain an old command-owned TUN plan.
/// Check before publishing either the new runtime config or source profile.
pub(crate) async fn require_capture_compatible(
    state: &AppState,
    next: &Value,
    id: Option<&str>,
) -> AppResult<()> {
    let app = lock(state.app_config(), "app_config")?.clone();
    let (current_app, current_source) = current(state)?;
    let previous = tun_params_for(&current_app, &current_source, current_app.tun.clone())?;
    let (next_app, next) = super::local_edits::resolve(&app, id, next)?;
    let candidate = tun_params_for(&next_app, &next, next_app.tun.clone())?;
    if previous == candidate {
        return Ok(());
    }
    let options = crate::services::core_config::ipc_options_from_app_config(&app.core);
    let tun = crate::kernel::zero::runtime::tun_status(Some(options)).await?;
    if tun.enabled {
        return Err(AppError::conflict(
            "tun",
            "configuration",
            "新配置的 TUN 参数与当前不同；请先关闭 TUN，再切换或更新配置，随后重新开启 TUN",
        ));
    }
    Ok(())
}

pub(crate) fn endpoint_for(app: &AppConfig, source: &Value) -> AppResult<(String, u16)> {
    let mut effective = source.clone();
    proxy_config::project_endpoint(&mut effective, &app.local_proxy, app.overrides.listener)?;
    proxy_config::extract_local_proxy(&effective)
        .map(|v| (v.host, v.port))
        .ok_or_else(|| {
            AppError::invalid_argument("当前配置没有可用的本地代理入口；请检查配置中的 inbounds")
        })
}

pub(crate) fn endpoint(state: &AppState) -> AppResult<(String, u16)> {
    let (app, source) = current(state)?;
    endpoint_for(&app, &source)
}

pub(crate) fn proxy_settings(state: &AppState) -> AppResult<(String, u16, Vec<String>)> {
    let app = lock(state.app_config(), "app_config")?.clone();
    proxy_settings_for(state, &app)
}

pub(crate) fn proxy_settings_for(
    state: &AppState,
    app: &AppConfig,
) -> AppResult<(String, u16, Vec<String>)> {
    let profile = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .cloned();
    let base = profile
        .as_ref()
        .and_then(|p| p.content.clone())
        .unwrap_or_else(|| json!({}));
    let (app, source) =
        super::local_edits::resolve(app, profile.as_ref().map(|p| p.id.as_str()), &base)?;
    let (host, port) = endpoint_for(&app, &source)?;
    // Arbitrary source bypass expressions remain enforced by Zero. Native
    // exceptions must never bypass a source policy using unrelated app defaults.
    let native = if !app.overrides.bypass && source.pointer("/route/bypass").is_some() {
        vec![]
    } else {
        app.local_proxy.bypass.clone()
    };
    Ok((host, port, native))
}

pub(crate) fn tun_params(state: &AppState, defaults: AppTunConfig) -> AppResult<Value> {
    let (app, source) = current(state)?;
    tun_params_for(&app, &source, defaults)
}

pub(crate) fn tun_params_for(
    app: &AppConfig,
    source: &Value,
    defaults: AppTunConfig,
) -> AppResult<Value> {
    let mut params = crate::kernel::zero::runtime::build_tun_start_params(defaults);
    if !app.overrides.tun {
        if let Some(value) = source.pointer("/runtime/tun") {
            let values = value
                .as_object()
                .ok_or_else(|| AppError::invalid_argument("runtime.tun must be an object"))?;
            params.as_object_mut().unwrap().extend(values.clone());
            // The compatibility mask follows the selected CIDR unless explicitly supplied.
            if values.contains_key("addr") && !values.contains_key("mask") {
                let mut compatible = app.tun.clone();
                compatible.addr = params["addr"].as_str().unwrap_or("").to_owned();
                crate::services::app_config::normalize_tun_mask(&mut compatible);
                params["mask"] = json!(compatible.mask);
            }
        }
    }
    let source_exclusions =
        !app.overrides.tun && source.pointer("/runtime/tun/exclude_cidrs").is_some();
    if app.overrides.bypass || !source_exclusions {
        let mut projected = app.clone();
        projected.tun.addr = params["addr"].as_str().unwrap_or("").to_owned();
        projected.tun.dual_stack = params["dual_stack"].as_bool().unwrap_or(true);
        bypass::normalize(&mut projected)?;
        let defaults = if !app.overrides.bypass && source.pointer("/route/bypass").is_some() {
            vec![]
        } else {
            projected.tun.exclude_cidrs
        };
        if app.overrides.bypass && source_exclusions {
            let existing = params["exclude_cidrs"].as_array_mut().ok_or_else(|| {
                AppError::invalid_argument("runtime.tun.exclude_cidrs must be an array")
            })?;
            for rule in defaults {
                let rule = json!(rule);
                if !existing.contains(&rule) {
                    existing.push(rule);
                }
            }
        } else {
            params["exclude_cidrs"] = json!(defaults);
        }
    }
    Ok(params)
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;

pub(crate) fn apply_bypass(config: &mut Value, source: &Value, app: &AppConfig) -> AppResult<()> {
    if !app.overrides.bypass && config.pointer("/route/bypass").is_some() {
        return Ok(());
    }
    // Supplying a bypass default must not rewrite an existing DNS or TUN block.
    let dns = source
        .pointer("/runtime/dns")
        .and_then(|_| config.pointer("/runtime/dns"))
        .cloned();
    let tun = config.pointer("/runtime/tun").cloned();
    bypass::apply(config, app)?;
    if !app.overrides.bypass {
        if let Some(dns) = dns {
            config["runtime"]["dns"] = dns;
        }
        if let Some(tun) = tun {
            config["runtime"]["tun"] = tun;
        }
    }
    Ok(())
}
