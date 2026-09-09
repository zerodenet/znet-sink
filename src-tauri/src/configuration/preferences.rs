//! Resolve client defaults without mutating a source profile or persisted defaults.
use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppTunConfig};
use crate::services::{bypass, common::lock, proxy_config};
use crate::state::app_state::AppState;
use serde_json::{json, Value};

pub(crate) fn source(state: &AppState) -> AppResult<Value> {
    Ok(lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone())
        .unwrap_or_else(|| json!({})))
}

pub(crate) fn describe(state: &AppState, app: &AppConfig) -> AppResult<Value> {
    let source = source(state)?;
    let origin = |force: bool, present: bool| {
        if force {
            "客户端显式覆盖"
        } else if present {
            "当前配置"
        } else {
            "客户端缺省值"
        }
    };
    let endpoint = endpoint_for(app, &source)
        .map(|(h, p)| format!("{h}:{p}"))
        .unwrap_or_else(|e| e.message);
    let url = if app.overrides.url_test {
        json!(app.url_test.url)
    } else {
        source
            .pointer("/runtime/latency_test_url")
            .cloned()
            .unwrap_or_else(|| json!(app.url_test.url))
    };
    let tun = tun_params_for(app, &source, app.tun.clone())
        .map(|tun| {
            format!(
                "{} · MTU {}；启停由客户端控制",
                tun["addr"].as_str().unwrap_or("无效地址"),
                tun["mtu"]
            )
        })
        .unwrap_or_else(|error| format!("配置无效：{}", error.message));
    let groups = source
        .get("outbound_groups")
        .and_then(Value::as_array)
        .map_or(0, |v| v.iter().filter(|g| g.get("url").is_some()).count());
    Ok(json!([
        {"key":"listener","label":"代理入口","source":origin(app.overrides.listener,source.get("inbounds").is_some()),"value":endpoint},
        {"key":"urlTest","label":"公共测速","source":origin(app.overrides.url_test,source.pointer("/runtime/latency_test_url").is_some()),"value":format!("{}；{} 个策略组另有专用地址",url.as_str().unwrap_or("配置值无效"),if app.overrides.url_test {0} else {groups})},
        {"key":"dns","label":"DNS","source":origin(app.overrides.dns,source.pointer("/runtime/dns").is_some()),"value":if !app.overrides.dns && source.pointer("/runtime/dns").is_some() {"保留配置中的 DNS 定义"} else if app.dns.enabled {"使用客户端 DNS 设置"} else {"关闭"}},
        {"key":"tun","label":"TUN 参数","source":origin(app.overrides.tun,source.pointer("/runtime/tun").is_some()),"value":tun},
        {"key":"bypass","label":"绕过规则","source":origin(app.overrides.bypass,source.pointer("/route/bypass").is_some()),"value":if !app.overrides.bypass && source.pointer("/route/bypass").is_some() {"保留配置中的绕过规则（含空列表）"} else {"使用客户端绕过设置"}},
        {"key":"rules","label":"通用规则追加","source":origin(app.overrides.rules,source.pointer("/route/rules").is_some()),"value":if !app.overrides.rules && source.pointer("/route/rules").is_some() {"保留配置规则，不追加客户端通用规则"} else if app.routing.inject_common_rules {"允许追加已启用的通用规则"} else {"关闭追加"}}
    ]))
}

pub(crate) fn owns_tun(state: &AppState) -> AppResult<bool> {
    let app = lock(state.app_config(), "app_config")?.clone();
    Ok(!app.overrides.tun
        && source(state)?
            .pointer("/runtime/tun")
            .is_some_and(Value::is_object))
}

/// A hot config apply cannot silently retain an old command-owned TUN plan.
/// Check before publishing either the new runtime config or source profile.
pub(crate) async fn require_capture_compatible(state: &AppState, next: &Value) -> AppResult<()> {
    let app = lock(state.app_config(), "app_config")?.clone();
    let previous = tun_params_for(&app, &source(state)?, app.tun.clone())?;
    let candidate = tun_params_for(&app, next, app.tun.clone())?;
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
    let app = lock(state.app_config(), "app_config")?.clone();
    endpoint_for(&app, &source(state)?)
}

pub(crate) fn proxy_settings(state: &AppState) -> AppResult<(String, u16, Vec<String>)> {
    let app = lock(state.app_config(), "app_config")?.clone();
    proxy_settings_for(state, &app)
}

pub(crate) fn proxy_settings_for(
    state: &AppState,
    app: &AppConfig,
) -> AppResult<(String, u16, Vec<String>)> {
    let source = source(state)?;
    let (host, port) = endpoint_for(app, &source)?;
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
    let app = lock(state.app_config(), "app_config")?.clone();
    tun_params_for(&app, &source(state)?, defaults)
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
