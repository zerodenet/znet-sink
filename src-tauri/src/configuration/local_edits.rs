//! Sparse, profile-owned edits. Original subscription documents stay untouched.
use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::services::{app_config, common::lock, proxy_config};
use crate::state::app_state::AppState;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) type Edits = BTreeMap<String, Value>;

pub(crate) fn active(state: &AppState) -> AppResult<(String, Value)> {
    lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .and_then(|p| p.content.clone().map(|v| (p.id.clone(), v)))
        .ok_or_else(|| AppError::invalid_argument("请先选择一份配置，再修改网络设置"))
}

pub(crate) fn resolve(
    app: &AppConfig,
    id: Option<&str>,
    base: &Value,
) -> AppResult<(AppConfig, Value)> {
    let mut app = app.clone();
    // Legacy choices are migrated at load/import; defaults cannot override profiles.
    app.overrides = Default::default();
    let mut source = base.clone();
    let edits = id
        .and_then(|id| app.profile_edits.get(id))
        .cloned()
        .unwrap_or_default();
    for (key, value) in &edits {
        match key.as_str() {
            "localProxy.host" | "localProxy.port" => {
                proxy_config::project_endpoint(&mut source, &app.local_proxy, false)?;
                let inbounds = source["inbounds"]
                    .as_array_mut()
                    .ok_or_else(|| AppError::invalid_argument("配置没有本地代理入口"))?;
                let entry = inbounds
                    .iter_mut()
                    .find(|v| {
                        matches!(
                            v.pointer("/protocol/type").and_then(Value::as_str),
                            Some("mixed" | "http" | "socks5")
                        )
                    })
                    .ok_or_else(|| {
                        AppError::invalid_argument(
                            "配置没有本地代理入口；本地修改不会自动添加监听器",
                        )
                    })?;
                let field = if key.ends_with("host") {
                    "address"
                } else {
                    "port"
                };
                entry["listen"][field] = value.clone();
            }
            "urlTest.url" => {
                object(&mut source, "runtime")?.insert("latency_test_url".into(), value.clone());
            }
            "urlTest.toleranceMs" => {
                if let Some(groups) = source
                    .get_mut("outbound_groups")
                    .and_then(Value::as_array_mut)
                {
                    for group in groups
                        .iter_mut()
                        .filter(|v| matches!(v["type"].as_str(), Some("url_test" | "urltest")))
                    {
                        group["tolerance_ms"] = value.clone();
                    }
                }
                app.url_test.tolerance_ms = value
                    .as_u64()
                    .ok_or_else(|| AppError::invalid_argument("invalid tolerance"))?;
            }
            "dns" => {
                app.dns = serde_json::from_value(value.clone()).map_err(invalid)?;
                app.overrides.dns = true;
                app.tun.dns_hijack = app.dns.dns_hijack;
                let runtime = object(&mut source, "runtime")?;
                let tun = runtime
                    .entry("tun")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or_else(|| AppError::invalid_argument("runtime.tun must be an object"))?;
                tun.insert("dns_hijack".into(), json!(app.dns.dns_hijack));
            }
            "bypass" => {
                app.bypass = Some(serde_json::from_value(value.clone()).map_err(invalid)?);
                crate::services::bypass::normalize(&mut app)?;
                app.overrides.bypass = true;
            }
            "routing.injectCommonRules" => {
                app.routing.inject_common_rules = value
                    .as_bool()
                    .ok_or_else(|| AppError::invalid_argument("invalid rule setting"))?;
                app.overrides.rules = true;
            }
            key if key.starts_with("tun.") => {
                let runtime = object(&mut source, "runtime")?;
                let tun = runtime
                    .entry("tun")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or_else(|| AppError::invalid_argument("runtime.tun must be an object"))?;
                tun.insert(snake(&key[4..]), value.clone());
            }
            _ => {
                return Err(AppError::invalid_argument(format!(
                    "不支持的本地修改字段：{key}"
                )))
            }
        }
    }
    Ok((app, source))
}

fn object<'a>(
    value: &'a mut Value,
    key: &str,
) -> AppResult<&'a mut serde_json::Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("配置必须为对象"))?
        .entry(key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument(format!("{key} must be an object")))
}
fn invalid(e: serde_json::Error) -> AppError {
    AppError::invalid_argument(e.to_string())
}
pub(crate) fn snake(key: &str) -> String {
    let mut result = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            result.push('_');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

pub(crate) fn candidate(
    previous: &AppConfig,
    id: &str,
    changes: Edits,
    reset: &[String],
) -> AppResult<AppConfig> {
    let mut next = previous.clone();
    let edits = next.profile_edits.entry(id.to_owned()).or_default();
    for key in reset {
        edits.remove(key);
    }
    for (key, value) in changes {
        validate_field(previous, &key, &value)?;
        edits.insert(key, value);
    }
    if edits.is_empty() {
        next.profile_edits.remove(id);
    }
    Ok(next)
}

fn validate_field(app: &AppConfig, key: &str, value: &Value) -> AppResult<()> {
    let (section, field) = key.split_once('.').unwrap_or((key, ""));
    let allowed = match section {
        "localProxy" => matches!(field, "host" | "port"),
        "urlTest" => matches!(field, "url" | "toleranceMs"),
        "tun" => matches!(
            field,
            "name"
                | "tag"
                | "addr"
                | "secondaryAddr"
                | "mtu"
                | "includeCidrs"
                | "excludeCidrs"
                | "dualStack"
                | "dnsHijack"
                | "autoRoute"
                | "strictRoute"
        ),
        "routing" => field == "injectCommonRules",
        "dns" | "bypass" => field.is_empty(),
        _ => false,
    };
    if !allowed {
        return Err(AppError::invalid_argument(format!(
            "不支持的本地修改字段：{key}"
        )));
    }
    let mut patch = json!({});
    patch[section] = if field.is_empty() {
        value.clone()
    } else {
        json!({field:value})
    };
    let patch: AppConfigPatch = serde_json::from_value(patch).map_err(invalid)?;
    app_config::prepare_update(app, patch)?;
    Ok(())
}

pub(crate) fn view(state: &AppState) -> AppResult<Value> {
    let (id, base) = active(state)?;
    let saved = lock(state.app_config(), "app_config")?.clone();
    let (app, source) = resolve(&saved, Some(&id), &base)?;
    let mut settings = serde_json::to_value(&app).map_err(invalid)?;
    let mut listener_view = source.clone();
    proxy_config::project_endpoint(&mut listener_view, &app.local_proxy, false)?;
    let listen = listener_view
        .get("inbounds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| {
            matches!(
                entry.pointer("/protocol/type").and_then(Value::as_str),
                Some("mixed" | "http" | "socks5")
            )
        })
        .and_then(|entry| entry.get("listen"));
    settings["localProxy"]["host"] = listen
        .and_then(|v| v.get("address"))
        .cloned()
        .unwrap_or_else(|| json!(""));
    settings["localProxy"]["port"] = listen
        .and_then(|v| v.get("port"))
        .cloned()
        .unwrap_or_else(|| json!(0));
    settings["urlTest"]["url"] = source
        .pointer("/runtime/latency_test_url")
        .cloned()
        .unwrap_or_else(|| json!(app.url_test.url));
    let tolerances: Vec<Value> = source
        .get("outbound_groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|group| matches!(group["type"].as_str(), Some("url_test" | "urltest")))
        .map(|group| {
            group
                .get("tolerance_ms")
                .cloned()
                .unwrap_or_else(|| json!(app.url_test.tolerance_ms))
        })
        .collect();
    if let Some(first) = tolerances
        .first()
        .filter(|first| tolerances.iter().all(|v| v == *first))
    {
        settings["urlTest"]["toleranceMs"] = first.clone();
    }
    let params = super::preferences::tun_params_for(&app, &source, app.tun.clone())?;
    if let Some(tun) = settings["tun"].as_object_mut() {
        for (key, value) in tun {
            if let Some(selected) = params.get(snake(key)) {
                *value = selected.clone();
            }
        }
    }
    if !app.overrides.dns {
        if let Some(dns) = source.pointer("/runtime/dns") {
            settings["dns"] = json!({"enabled":true,"config":dns,"dnsHijack":params["dns_hijack"]});
        }
    }
    let edited = saved.profile_edits.get(&id).cloned().unwrap_or_default();
    Ok(
        json!({"profileId":id,"settings":settings,"editedFields":edited.keys().collect::<Vec<_>>(),
        "sourceEndpoint":super::preferences::endpoint_for(&saved,&base).ok(),"sourceBypass":base.pointer("/route/bypass"),"groupTolerances":tolerances}),
    )
}

#[cfg(test)]
#[path = "local_edits_tests.rs"]
mod tests;

/// Preserve the previous UI's explicit choices on the active profile only.
/// Portable defaults themselves are never inferred to be edits.
pub(crate) fn migrate_legacy(app: &mut AppConfig, id: Option<&str>) -> bool {
    if app.overrides == Default::default() {
        return false;
    }
    let Some(id) = id else {
        return false;
    };
    let flags = app.overrides.clone();
    let mut values = Edits::new();
    if flags.listener {
        values.insert("localProxy.host".into(), json!(app.local_proxy.host));
        values.insert("localProxy.port".into(), json!(app.local_proxy.port));
    }
    if flags.url_test {
        values.insert("urlTest.url".into(), json!(app.url_test.url));
        values.insert(
            "urlTest.toleranceMs".into(),
            json!(app.url_test.tolerance_ms),
        );
    }
    if flags.dns {
        values.insert("dns".into(), json!(app.dns));
    }
    if flags.bypass {
        if let Some(bypass) = &app.bypass {
            values.insert("bypass".into(), json!(bypass));
        }
    }
    if flags.rules {
        values.insert(
            "routing.injectCommonRules".into(),
            json!(app.routing.inject_common_rules),
        );
    }
    if flags.tun {
        if let Value::Object(tun) = json!(app.tun) {
            for (field, value) in tun {
                if matches!(
                    field.as_str(),
                    "name"
                        | "tag"
                        | "addr"
                        | "secondaryAddr"
                        | "mtu"
                        | "includeCidrs"
                        | "excludeCidrs"
                        | "dualStack"
                        | "dnsHijack"
                        | "autoRoute"
                        | "strictRoute"
                ) {
                    values.insert(format!("tun.{field}"), value);
                }
            }
        }
    }
    let edits = app.profile_edits.entry(id.to_owned()).or_default();
    for (key, value) in values {
        edits.entry(key).or_insert(value);
    }
    app.overrides = Default::default();
    true
}
