use std::fs;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use zero_rule::zrs::{verify, VerifyMode};

use crate::errors::{AppError, AppResult};
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::app_config::AppDnsConfig;
use crate::models::core_process::CoreProcessState;
use crate::models::gui_core::GuiProxyMode;
use crate::models::rule_set::{
    CommonRuleAction, CommonRuleBindingInput, CommonRuleInjectionStatus, EffectiveRuleSetOption,
    RuleSetProfile,
};
use crate::services::{
    app_config, common, core_config, core_process, domain_store, policy_selection, proxy_mode,
    rule_set, url_test,
};
use crate::state::app_state::AppState;

const COMMON_TAG_PREFIX: &str = "gui-common-";

pub fn compose_effective_config(state: &AppState, base: &Value) -> AppResult<Value> {
    let enabled = common::lock(state.app_config(), "app_config")?
        .routing
        .inject_common_rules;
    let profiles = common::lock(state.rule_sets(), "rule_set")?.clone();
    compose_effective_with(state, base, enabled, &profiles, None)
}

pub(crate) fn compose_effective_config_with_dns(
    state: &AppState,
    base: &Value,
    dns: &AppDnsConfig,
) -> AppResult<Value> {
    let enabled = common::lock(state.app_config(), "app_config")?
        .routing
        .inject_common_rules;
    let profiles = common::lock(state.rule_sets(), "rule_set")?.clone();
    compose_effective_with(state, base, enabled, &profiles, Some(dns))
}

fn compose_effective_with(
    state: &AppState,
    base: &Value,
    enabled: bool,
    profiles: &[RuleSetProfile],
    dns_override: Option<&AppDnsConfig>,
) -> AppResult<Value> {
    let config = compose_with(base, enabled, profiles)?.config;
    finalize_effective_config(state, base, config, dns_override)
}

fn finalize_effective_config(
    state: &AppState,
    base: &Value,
    mut config: Value,
    dns_override: Option<&AppDnsConfig>,
) -> AppResult<Value> {
    apply_global_dns(state, &mut config, dns_override)?;
    let tolerance_ms = common::lock(state.app_config(), "app_config")?
        .url_test
        .tolerance_ms;
    if url_test::supports_tolerance(state) {
        url_test::apply_default_tolerance(&mut config, tolerance_ms)?;
    }
    policy_selection::apply_saved_selections(state, base, &mut config)?;
    Ok(config)
}

/// DNS/Fake-IP is a client-owned runtime concern rather than part of a proxy
/// profile. Remove any legacy profile-owned value and inject the persisted
/// global setting into the effective config sent to Zero.
fn apply_global_dns(
    state: &AppState,
    config: &mut Value,
    dns_override: Option<&AppDnsConfig>,
) -> AppResult<()> {
    let app_dns = match dns_override {
        Some(dns) => dns.clone(),
        None => common::lock(state.app_config(), "app_config")?.dns.clone(),
    };
    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("proxy config must be a JSON object"))?;
    let runtime = root
        .entry("runtime".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let runtime = runtime
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("runtime must be a JSON object"))?;
    runtime.remove("dns");

    if app_dns.enabled {
        let dns = app_dns.config.ok_or_else(|| {
            AppError::invalid_argument("global DNS is enabled without a DNS configuration")
        })?;
        runtime.insert(
            "dns".to_string(),
            serde_json::to_value(dns).map_err(|error| {
                AppError::internal(format!("failed to serialize global DNS config: {error}"))
            })?,
        );
    } else if runtime.is_empty() {
        root.remove("runtime");
    }
    Ok(())
}

pub(crate) fn strip_profile_dns(config: &mut Value) {
    let Some(root) = config.as_object_mut() else {
        return;
    };
    let remove_runtime = root
        .get_mut("runtime")
        .and_then(Value::as_object_mut)
        .map(|runtime| {
            runtime.remove("dns");
            runtime.is_empty()
        })
        .unwrap_or(false);
    if remove_runtime {
        root.remove("runtime");
    }
}

pub fn status(state: &AppState) -> AppResult<CommonRuleInjectionStatus> {
    let enabled = common::lock(state.app_config(), "app_config")?
        .routing
        .inject_common_rules;
    let profiles = common::lock(state.rule_sets(), "rule_set")?.clone();
    let active = common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone());
    let Some(base) = active else {
        return Ok(CommonRuleInjectionStatus {
            enabled,
            effective: false,
            mode: None,
            eligible_count: eligible_profiles(&profiles).count(),
            injected_count: 0,
            reason: Some("当前没有活动配置".to_string()),
        });
    };
    match compose_with(&base, enabled, &profiles) {
        Ok(result) => Ok(CommonRuleInjectionStatus {
            enabled,
            effective: result.injected_count > 0,
            mode: result.mode,
            eligible_count: eligible_profiles(&profiles).count(),
            injected_count: result.injected_count,
            reason: result.reason,
        }),
        Err(error) => Ok(CommonRuleInjectionStatus {
            enabled,
            effective: false,
            mode: proxy_mode::detect_route_mode(&base).map(|detected| match detected.mode {
                GuiProxyMode::Global => "global".to_string(),
                GuiProxyMode::Rule => "rule".to_string(),
                GuiProxyMode::Direct => "direct".to_string(),
            }),
            eligible_count: eligible_profiles(&profiles).count(),
            injected_count: 0,
            reason: Some(error.message),
        }),
    }
}

pub fn effective_rule_set_options(state: &AppState) -> AppResult<Vec<EffectiveRuleSetOption>> {
    let Some(config) = current_effective_config(state)? else {
        return Ok(Vec::new());
    };
    let profiles = common::lock(state.rule_sets(), "rule_set")?;
    let definitions = config
        .pointer("/route/rule_sets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    let mut options = definitions
        .into_iter()
        .filter_map(|definition| {
            let tag = definition.get("tag")?.as_str()?.trim();
            if tag.is_empty() || !seen.insert(tag.to_string()) {
                return None;
            }
            let path = definition.get("path").and_then(Value::as_str);
            let profile = profiles.iter().find(|profile| {
                common_tag(&profile.id) == tag
                    || path.is_some_and(|path| {
                        profile
                            .artifact
                            .as_ref()
                            .is_some_and(|artifact| artifact.path == path)
                    })
            });
            let (name, source) = profile.map_or_else(
                || (tag.to_string(), "profile".to_string()),
                |profile| {
                    let source = if profile.managed_by_subscription_id.is_some() {
                        "subscription"
                    } else if profile.built_in {
                        "builtin"
                    } else if profile.source.is_some() {
                        "remote"
                    } else {
                        "local"
                    };
                    (profile.name.clone(), source.to_string())
                },
            );
            Some(EffectiveRuleSetOption {
                tag: tag.to_string(),
                name,
                source,
            })
        })
        .collect::<Vec<_>>();
    options.sort_by(|left, right| left.name.cmp(&right.name).then(left.tag.cmp(&right.tag)));
    Ok(options)
}

pub async fn set_enabled(
    app_handle: AppHandle,
    enabled: bool,
) -> AppResult<CommonRuleInjectionStatus> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    if previous.routing.inject_common_rules == enabled {
        return status(state.inner());
    }
    let mut next = previous.clone();
    next.routing.inject_common_rules = enabled;
    let base = active_content(state.inner())?;
    let profiles = common::lock(state.rule_sets(), "rule_set")?.clone();
    let previous_effective = base
        .as_ref()
        .map(|base| {
            compose_effective_with(
                state.inner(),
                base,
                previous.routing.inject_common_rules,
                &profiles,
                None,
            )
        })
        .transpose()?;
    let next_effective = base
        .as_ref()
        .map(|base| compose_effective_with(state.inner(), base, enabled, &profiles, None))
        .transpose()?;

    apply_if_running(state.inner(), next_effective.clone()).await?;
    if let Err(error) = app_config::replace(state.inner(), next) {
        let _ = apply_if_running(state.inner(), previous_effective.clone()).await;
        return Err(error);
    }
    if base.is_some() {
        if let Err(error) = core_config::export_active(state.clone()) {
            let _ = app_config::replace(state.inner(), previous);
            let _ = apply_if_running(state.inner(), previous_effective).await;
            let _ = core_config::export_active(state.clone());
            return Err(error);
        }
    }
    status(state.inner())
}

pub async fn set_binding(
    app_handle: AppHandle,
    input: CommonRuleBindingInput,
) -> AppResult<RuleSetProfile> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    let id = common::normalize_required(input.rule_set_id, "ruleSetId")?;
    let previous = common::lock(state.rule_sets(), "rule_set")?.clone();
    let mut next = previous.clone();
    let profile = next
        .iter_mut()
        .find(|profile| profile.id == id)
        .ok_or_else(|| AppError::not_found("rule_set", id.clone()))?;
    if profile.managed_by_subscription_id.is_some()
        || rule_set::is_managed_subscription_rule_set_id(&profile.id)
    {
        return Err(AppError::invalid_argument(
            "subscription-managed rules cannot be bound as common rules",
        ));
    }
    if input.enabled && profile.artifact.is_none() {
        return Err(AppError::invalid_argument(
            "the common rule has no verified ZRS artifact",
        ));
    }
    profile.common_binding = Some(crate::models::rule_set::CommonRuleBinding {
        enabled: input.enabled,
        action: input.action,
        order: input.order,
    });
    let updated = profile.clone();

    let base = active_content(state.inner())?;
    let inject_enabled = common::lock(state.app_config(), "app_config")?
        .routing
        .inject_common_rules;
    let old_effective = base
        .as_ref()
        .map(|base| compose_effective_with(state.inner(), base, inject_enabled, &previous, None))
        .transpose()?;
    let next_effective = base
        .as_ref()
        .map(|base| compose_effective_with(state.inner(), base, inject_enabled, &next, None))
        .transpose()?;
    apply_if_running(state.inner(), next_effective).await?;
    if let Err(error) = domain_store::save_rule_sets(&next) {
        let _ = apply_if_running(state.inner(), old_effective.clone()).await;
        return Err(error);
    }
    *common::lock(state.rule_sets(), "rule_set")? = next;
    if base.is_some() {
        if let Err(error) = core_config::export_active(state.clone()) {
            let _ = domain_store::save_rule_sets(&previous);
            *common::lock(state.rule_sets(), "rule_set")? = previous;
            let _ = apply_if_running(state.inner(), old_effective).await;
            let _ = core_config::export_active(state.clone());
            return Err(error);
        }
    }
    Ok(updated)
}

/// Recompose after a rule asset generation changes. The base profile remains
/// untouched; failure leaves the kernel on its last accepted configuration.
pub async fn reconcile_after_rule_change(app_handle: AppHandle) -> AppResult<()> {
    let state = app_handle.state::<AppState>();
    let _operation = state.proxy_config_operation().lock().await;
    reconcile_current_config_locked(app_handle.clone()).await
}

/// Recompose and publish the current effective configuration while the caller
/// holds `proxy_config_operation`.
///
/// Startup uses this after adopting an already-running kernel. That kernel may
/// still have the configuration from the previous GUI process, including an
/// older set of built-in/common rules.
pub(crate) async fn reconcile_current_config_locked(app_handle: AppHandle) -> AppResult<()> {
    let state = app_handle.state::<AppState>();
    let effective = current_effective_config(state.inner())?;
    let has_active_config = effective.is_some();
    apply_if_running(state.inner(), effective).await?;
    if has_active_config {
        core_config::export_active(state.clone())?;
    }
    Ok(())
}

fn current_effective_config(state: &AppState) -> AppResult<Option<Value>> {
    active_content(state)?
        .as_ref()
        .map(|base| compose_effective_config(state, base))
        .transpose()
}

struct ComposeResult {
    config: Value,
    injected_count: usize,
    mode: Option<String>,
    reason: Option<String>,
}

fn compose_with(
    base: &Value,
    enabled: bool,
    profiles: &[RuleSetProfile],
) -> AppResult<ComposeResult> {
    let mut config = base.clone();
    strip_common_overlay(&mut config)?;
    prune_undefined_rule_set_rules(&mut config)?;
    let mode = proxy_mode::detect_route_mode(base).map(|detected| match detected.mode {
        GuiProxyMode::Global => "global".to_string(),
        GuiProxyMode::Rule => "rule".to_string(),
        GuiProxyMode::Direct => "direct".to_string(),
    });
    if !enabled {
        return Ok(ComposeResult {
            config,
            injected_count: 0,
            mode,
            reason: Some("公共规则注入已关闭".to_string()),
        });
    }
    if mode.as_deref() != Some("rule") {
        return Ok(ComposeResult {
            config,
            injected_count: 0,
            mode,
            reason: Some("等待活动配置切换到规则模式".to_string()),
        });
    }

    let mut selected = eligible_profiles(profiles).collect::<Vec<_>>();
    selected.sort_by_key(|profile| {
        let binding = profile
            .common_binding
            .as_ref()
            .expect("eligible profile has binding");
        (binding.order, profile.id.as_str())
    });
    if selected.is_empty() {
        return Ok(ComposeResult {
            config,
            injected_count: 0,
            mode,
            reason: Some("尚未启用公共规则".to_string()),
        });
    }

    let root = config
        .as_object_mut()
        .ok_or_else(|| AppError::invalid_argument("proxy config must be a JSON object"))?;
    let route = object_field(root, "route")?;
    let final_action = route
        .get("final")
        .cloned()
        .unwrap_or_else(|| json!({ "type": "direct" }));
    let rule_sets = array_field(route, "rule_sets")?;
    let mut definitions = Vec::with_capacity(selected.len());
    let mut rules = Vec::with_capacity(selected.len());
    for profile in selected {
        let artifact = profile
            .artifact
            .as_ref()
            .expect("eligible profile has artifact");
        verify_artifact(profile, artifact)?;
        let tag = common_tag(&profile.id);
        definitions
            .push(json!({ "tag": tag, "type": "file", "path": artifact.path, "format": "zrs" }));
        let action = match &profile
            .common_binding
            .as_ref()
            .expect("eligible profile has binding")
            .action
        {
            CommonRuleAction::Final => final_action.clone(),
            CommonRuleAction::Proxy => json!({
                "type": "route",
                "outbound": proxy_mode::resolve_global_outbound(base, None)
            }),
            CommonRuleAction::Direct => json!({ "type": "direct" }),
            CommonRuleAction::Reject => json!({ "type": "reject" }),
        };
        rules.push(json!({ "condition": { "type": "rule_set", "tag": tag }, "action": action }));
    }
    let injected_count = rules.len();
    rule_sets.extend(definitions);
    let existing_rules = array_field(route, "rules")?;
    // Subscription rules are usually more specific than GUI-wide rules
    // (for example, an AI service group versus the broad built-in GFW
    // domain set). Preserve those semantics by evaluating the subscription
    // first and using common rules only as a fallback before `route.final`.
    existing_rules.extend(rules);
    Ok(ComposeResult {
        config,
        injected_count,
        mode,
        reason: None,
    })
}

fn eligible_profiles(profiles: &[RuleSetProfile]) -> impl Iterator<Item = &RuleSetProfile> {
    profiles.iter().filter(|profile| {
        profile.enabled
            && profile.managed_by_subscription_id.is_none()
            && !rule_set::is_managed_subscription_rule_set_id(&profile.id)
            && profile.artifact.is_some()
            && profile
                .common_binding
                .as_ref()
                .is_some_and(|binding| binding.enabled)
    })
}

fn verify_artifact(
    profile: &RuleSetProfile,
    artifact: &crate::models::rule_set::ZrsArtifact,
) -> AppResult<()> {
    let bytes = fs::read(&artifact.path).map_err(|error| {
        AppError::invalid_argument(format!(
            "公共规则 '{}' 的 ZRS 文件不可读: {error}",
            profile.name
        ))
    })?;
    let metadata = verify(&bytes, VerifyMode::FullChecksum).map_err(|error| {
        AppError::invalid_argument(format!(
            "公共规则 '{}' 的 ZRS 校验失败: {error}",
            profile.name
        ))
    })?;
    if metadata.body_checksum != artifact.checksum {
        return Err(AppError::invalid_argument(format!(
            "公共规则 '{}' 的 ZRS 校验值不匹配",
            profile.name
        )));
    }
    Ok(())
}

fn common_tag(id: &str) -> String {
    let digest = Sha256::digest(id.as_bytes());
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{COMMON_TAG_PREFIX}{suffix}")
}

fn strip_common_overlay(config: &mut Value) -> AppResult<()> {
    let Some(root) = config.as_object_mut() else {
        return Err(AppError::invalid_argument(
            "proxy config must be a JSON object",
        ));
    };
    let Some(route) = root.get_mut("route").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    if let Some(items) = route.get_mut("rule_sets").and_then(Value::as_array_mut) {
        items.retain(|item| {
            !item
                .get("tag")
                .and_then(Value::as_str)
                .is_some_and(|tag| tag.starts_with(COMMON_TAG_PREFIX))
        });
    }
    if let Some(items) = route.get_mut("rules").and_then(Value::as_array_mut) {
        items.retain(|item| {
            !item
                .get("condition")
                .and_then(|condition| condition.get("tag"))
                .and_then(Value::as_str)
                .is_some_and(|tag| tag.starts_with(COMMON_TAG_PREFIX))
        });
    }
    Ok(())
}

fn prune_undefined_rule_set_rules(config: &mut Value) -> AppResult<Vec<String>> {
    let Some(root) = config.as_object_mut() else {
        return Err(AppError::invalid_argument(
            "proxy config must be a JSON object",
        ));
    };
    let Some(route) = root.get_mut("route").and_then(Value::as_object_mut) else {
        return Ok(Vec::new());
    };
    let defined = route
        .get("rule_sets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("tag").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>();
    let mut removed = std::collections::BTreeSet::new();
    if let Some(rules) = route.get_mut("rules").and_then(Value::as_array_mut) {
        rules.retain(|rule| {
            let referenced = rule
                .get("condition")
                .filter(|condition| {
                    condition.get("type").and_then(Value::as_str) == Some("rule_set")
                })
                .and_then(|condition| condition.get("tag"))
                .and_then(Value::as_str);
            let keep = referenced.is_none_or(|tag| defined.contains(tag));
            if !keep {
                removed.insert(
                    referenced
                        .expect("missing rule-set tag is retained")
                        .to_string(),
                );
            }
            keep
        });
    }
    Ok(removed.into_iter().collect())
}

fn object_field<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> AppResult<&'a mut Map<String, Value>> {
    if !root.get(key).is_some_and(Value::is_object) {
        root.insert(key.to_string(), Value::Object(Map::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AppError::invalid_argument(format!("{key} must be an object")))
}

fn array_field<'a>(root: &'a mut Map<String, Value>, key: &str) -> AppResult<&'a mut Vec<Value>> {
    if !root.contains_key(key) {
        root.insert(key.to_string(), Value::Array(Vec::new()));
    }
    root.get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::invalid_argument(format!("route.{key} must be an array")))
}

fn active_content(state: &AppState) -> AppResult<Option<Value>> {
    Ok(common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .and_then(|profile| profile.content.clone()))
}

async fn apply_if_running(state: &AppState, config: Option<Value>) -> AppResult<()> {
    if core_process::refresh_status(state)?.state != CoreProcessState::Running {
        return Ok(());
    }
    let config = config.ok_or_else(|| {
        AppError::invalid_argument("a running kernel requires an active proxy config")
    })?;
    let core = common::lock(state.app_config(), "app_config")?.core.clone();
    ZeroAdapter::new()
        .apply_config(config, core_config::ipc_options_from_app_config(&core))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::app_config::AppConfig;
    use crate::models::proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile};
    use crate::models::rule_set::{CommonRuleBinding, ZrsArtifact};
    use crate::state::app_state::AppState;
    use zero_rule::protocol::decode_json;
    use zero_rule::zrs::encode;
    use zero_rule::RuleSetCompiler;

    fn profile(id: &str, order: u32, action: CommonRuleAction) -> RuleSetProfile {
        RuleSetProfile {
            id: id.into(),
            name: id.into(),
            enabled: true,
            built_in: false,
            provenance: None,
            managed_by_subscription_id: None,
            common_binding: Some(CommonRuleBinding {
                enabled: true,
                action,
                order,
            }),
            semantic_ir: json!({"version":1,"rules":[]}),
            source: None,
            source_state: Default::default(),
            artifact: Some(ZrsArtifact {
                path: "missing.zrs".into(),
                major_version: 1,
                minor_version: 0,
                checksum: 0,
                file_size: 0,
                entry_count: 0,
                built_at_unix_ms: 0,
            }),
            updated_at_unix_ms: 0,
            last_sync_at_unix_ms: None,
            last_error: None,
        }
    }

    #[test]
    fn disabled_or_non_rule_mode_never_injects() {
        let profiles = vec![profile("one", 0, CommonRuleAction::Direct)];
        let base = json!({"mode":{"type":"global","outbound":"proxy"},"route":{"rules":[]}});
        assert_eq!(
            compose_with(&base, true, &profiles).unwrap().injected_count,
            0
        );
        let base = json!({"mode":{"type":"rule"},"route":{"rules":[]}});
        assert_eq!(
            compose_with(&base, false, &profiles)
                .unwrap()
                .injected_count,
            0
        );
    }

    #[test]
    fn global_dns_is_injected_and_profile_dns_is_discarded() {
        let mut app_config = AppConfig::default();
        app_config.dns.enabled = true;
        app_config.dns.config = Some(
            serde_json::from_value(json!({
                "servers": { "system": { "type": "system" } },
                "default_server": "system",
                "answer": { "type": "fake_ip", "cidr": "198.18.0.0/15", "ttl_seconds": 60 }
            }))
            .unwrap(),
        );
        let state =
            AppState::with_domain_data(app_config, Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let base = json!({
            "mode": {"type": "global", "outbound": "proxy"},
            "runtime": {
                "dns": {"servers": {"stale": {"type": "system"}}, "default_server": "stale"},
                "tun": {"dns_hijack": true}
            }
        });

        let effective = compose_effective_config(&state, &base).unwrap();
        assert_eq!(effective["runtime"]["dns"]["default_server"], "system");
        assert_eq!(effective["runtime"]["dns"]["answer"]["type"], "fake_ip");
        assert_eq!(effective["runtime"]["tun"]["dns_hijack"], true);
        assert!(effective["runtime"]["dns"]["servers"]
            .get("stale")
            .is_none());
    }

    fn verified_profile(
        id: &str,
        order: u32,
        action: CommonRuleAction,
    ) -> (RuleSetProfile, std::path::PathBuf) {
        let source = decode_json(
            serde_json::to_string(&json!({
                "version": 1,
                "name": id,
                "rules": [{"type":"domain_exact","value":format!("{id}.example.com")}]
            }))
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
        let (compiled, _) = RuleSetCompiler.compile(source).unwrap();
        let bytes = encode(&compiled).unwrap();
        let metadata = verify(&bytes, VerifyMode::FullChecksum).unwrap();
        let path = std::env::temp_dir().join(format!(
            "znet-common-rule-{id}-{}-{}.zrs",
            std::process::id(),
            common::now_unix_ms()
        ));
        fs::write(&path, &bytes).unwrap();
        let mut profile = profile(id, order, action);
        profile.artifact = Some(ZrsArtifact {
            path: path.to_string_lossy().into_owned(),
            major_version: metadata.major_version,
            minor_version: metadata.minor_version,
            checksum: metadata.body_checksum,
            file_size: metadata.file_size,
            entry_count: metadata.entry_count(),
            built_at_unix_ms: 0,
        });
        (profile, path)
    }

    #[test]
    fn common_rules_follow_subscription_rules_in_binding_order_and_composition_is_idempotent() {
        let (later, later_path) = verified_profile("later", 20, CommonRuleAction::Reject);
        let (first, first_path) = verified_profile("first", 10, CommonRuleAction::Direct);
        let base = json!({
            "mode":{"type":"rule"},
            "route":{
                "rule_sets":[{"tag":"airport","type":"file","path":"airport.zrs","format":"zrs"}],
                "rules":[{"condition":{"type":"rule_set","tag":"airport"},"action":{"type":"route","outbound":"proxy"}}],
                "final":{"type":"route","outbound":"proxy"}
            }
        });
        let composed = compose_with(&base, true, &[later, first]).unwrap();
        assert_eq!(composed.injected_count, 2);
        let rules = composed.config["route"]["rules"].as_array().unwrap();
        assert_eq!(rules[0]["condition"]["tag"], "airport");
        assert_eq!(rules[1]["action"]["type"], "direct");
        assert_eq!(rules[2]["action"]["type"], "reject");
        let recomposed = compose_with(&composed.config, true, &[]).unwrap();
        assert_eq!(
            recomposed.config["route"]["rules"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            recomposed.config["route"]["rule_sets"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let _ = fs::remove_file(later_path);
        let _ = fs::remove_file(first_path);
    }

    #[test]
    fn startup_reconcile_uses_newly_loaded_common_rules() {
        let (profile, path) = verified_profile("new-default", 10, CommonRuleAction::Direct);
        let base = json!({
            "mode":{"type":"rule"},
            "route":{"rule_sets":[],"rules":[],"final":{"type":"direct"}}
        });
        let state = AppState::with_domain_data(
            AppConfig::default(),
            vec![ProxyConfigProfile {
                id: "active".into(),
                name: "Active".into(),
                kernel: "zero".into(),
                format: "zero-json".into(),
                path: None,
                content: Some(base),
                active: true,
                updated_at_unix_ms: 0,
                capabilities: ProxyConfigCapabilities::default(),
            }],
            Vec::new(),
            vec![profile],
            Vec::new(),
        );

        let effective = current_effective_config(&state).unwrap().unwrap();
        let expected_tag = common_tag("new-default");

        assert_eq!(effective["route"]["rule_sets"][0]["tag"], expected_tag);
        assert_eq!(
            effective["route"]["rules"][0]["condition"]["tag"],
            common_tag("new-default")
        );
        let options = effective_rule_set_options(&state).unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].tag, expected_tag);
        assert_eq!(options[0].name, "new-default");
        assert_eq!(options[0].source, "local");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn effective_options_keep_subscription_assets_read_only_but_selectable_by_real_tag() {
        let (mut profile, path) = verified_profile("managed", 0, CommonRuleAction::Direct);
        profile.name = "Airport / AI-Suite".into();
        profile.managed_by_subscription_id = Some("subscription-1".into());
        profile.common_binding = None;
        let base = json!({
            "mode":{"type":"rule"},
            "route":{
                "rule_sets":[{"tag":"AI-Suite","type":"file","path":path,"format":"zrs"}],
                "rules":[],
                "final":{"type":"direct"}
            }
        });
        let state = AppState::with_domain_data(
            AppConfig::default(),
            vec![ProxyConfigProfile {
                id: "active".into(),
                name: "Active".into(),
                kernel: "zero".into(),
                format: "zero-json".into(),
                path: None,
                content: Some(base),
                active: true,
                updated_at_unix_ms: 0,
                capabilities: ProxyConfigCapabilities::default(),
            }],
            Vec::new(),
            vec![profile],
            Vec::new(),
        );

        let options = effective_rule_set_options(&state).unwrap();

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].tag, "AI-Suite");
        assert_eq!(options[0].name, "Airport / AI-Suite");
        assert_eq!(options[0].source, "subscription");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn proxy_binding_routes_to_the_resolved_proxy_group() {
        let (profile, path) = verified_profile("gfw", 40, CommonRuleAction::Proxy);
        let base = json!({
            "mode":{"type":"rule"},
            "outbound_groups":[{"tag":"Auto","type":"url_test","outbounds":["node"]}],
            "route":{"rule_sets":[],"rules":[],"final":{"type":"direct"}}
        });

        let composed = compose_with(&base, true, &[profile]).unwrap();

        assert_eq!(
            composed.config["route"]["rules"][0]["action"]["type"],
            "route"
        );
        assert_eq!(
            composed.config["route"]["rules"][0]["action"]["outbound"],
            "Auto"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn subscription_managed_assets_are_never_eligible() {
        let (mut managed, path) = verified_profile("managed", 0, CommonRuleAction::Direct);
        managed.managed_by_subscription_id = Some("subscription-1".into());
        assert_eq!(eligible_profiles(&[managed]).count(), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn undefined_rule_set_references_are_removed_without_touching_valid_rules() {
        let base = json!({
            "mode":{"type":"rule"},
            "route":{
                "rule_sets":[{"tag":"available","type":"file","path":"available.zrs","format":"zrs"}],
                "rules":[
                    {"condition":{"type":"rule_set","tag":"missing"},"action":{"type":"reject"}},
                    {"condition":{"type":"rule_set","tag":"available"},"action":{"type":"direct"}},
                    {"condition":{"type":"domain","values":["example.com"]},"action":{"type":"direct"}}
                ]
            }
        });
        let composed = compose_with(&base, false, &[]).unwrap();
        let rules = composed.config["route"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["condition"]["tag"], "available");
        assert_eq!(rules[1]["condition"]["type"], "domain");
    }
}
