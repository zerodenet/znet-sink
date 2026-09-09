#[cfg(test)]
use crate::models::rule_set::CommonRuleAction;
use std::fs;

#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use zero_rule::zrs::{verify, VerifyMode};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppDnsConfig};
use crate::models::core_process::CoreProcessState;
#[cfg(test)]
use crate::models::dns_config::CLIENT_DNS_DETOUR_ROUTE_FINAL;
use crate::models::gui_core::GuiProxyMode;
use crate::models::rule_set::{
    CommonRuleBindingInput, CommonRuleInjectionStatus, EffectiveRuleSetOption, RuleSetProfile,
};
use crate::services::{
    common, core_config, core_process, domain_store, policy_selection, proxy_mode, rule_set,
    url_test,
};
use crate::state::app_state::AppState;

pub fn compose_effective_config(state: &AppState, base: &Value) -> AppResult<Value> {
    let (id, _) = policy_selection::saved_selections(state, base)?;
    compose_effective_config_for(state, base, id.as_deref())
}

pub(crate) fn compose_effective_config_for(
    state: &AppState,
    base: &Value,
    id: Option<&str>,
) -> AppResult<Value> {
    let app = common::lock(state.app_config(), "app_config")?.clone();
    let profiles = common::lock(state.rule_sets(), "rule_set")?.clone();
    let (app, edited) = crate::configuration::local_edits::resolve(&app, id, base)?;
    let config = compose_rules_for(
        &edited,
        app.routing.inject_common_rules,
        &profiles,
        app.overrides.rules,
    )?;
    let (_, selections) = policy_selection::saved_selections(state, base)?;
    let inputs = crate::configuration::composition::Inputs {
        source_profile_id: id.map(str::to_owned),
        app,
        supports_tolerance: url_test::supports_tolerance(state),
        selections,
    };
    let mut candidate = crate::configuration::composition::finalize(base, config, &inputs)?;
    if id
        .and_then(|id| inputs.app.profile_edits.get(id))
        .is_some_and(|edits| !edits.is_empty())
    {
        candidate.report.compare(
            "profile_local_edits",
            &[
                "/inbounds",
                "/runtime/latency_test_url",
                "/runtime/tun",
                "/outbound_groups",
            ],
            base,
            &edited,
        );
    }
    state.configuration().record(candidate.report.clone());
    Ok(candidate.config)
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
    compose_effective_with(state, base, enabled, &profiles, Some(dns), None)
}

pub(crate) fn validate_app_config_candidate(
    state: &AppState,
    app_config: &AppConfig,
) -> AppResult<()> {
    let Some(base) = active_content(state)? else {
        return Ok(());
    };
    let profiles = common::lock(state.rule_sets(), "rule_set")?.clone();
    let (id, _) = policy_selection::saved_selections(state, &base)?;
    let (scoped, edited) =
        crate::configuration::local_edits::resolve(app_config, id.as_deref(), &base)?;
    let enabled = scoped.routing.inject_common_rules
        && (scoped.overrides.rules || edited.pointer("/route/rules").is_none());
    let config = compose_rules_for(&edited, enabled, &profiles, scoped.overrides.rules)?;
    finalize_effective_config(
        state,
        &base,
        config,
        Some(&app_config.dns),
        Some(app_config.url_test.tolerance_ms),
        Some(app_config),
    )?;
    Ok(())
}

fn compose_effective_with(
    state: &AppState,
    base: &Value,
    enabled: bool,
    profiles: &[RuleSetProfile],
    dns_override: Option<&AppDnsConfig>,
    tolerance_override: Option<u64>,
) -> AppResult<Value> {
    let (scoped, _) = crate::configuration::preferences::current(state)?;
    let enabled = if scoped.overrides.rules {
        scoped.routing.inject_common_rules
    } else {
        enabled
    };
    let config = compose_rules_for(base, enabled, profiles, scoped.overrides.rules)?;
    finalize_effective_config(state, base, config, dns_override, tolerance_override, None)
}

fn finalize_effective_config(
    state: &AppState,
    base: &Value,
    config: Value,
    dns_override: Option<&AppDnsConfig>,
    tolerance_override: Option<u64>,
    app_override: Option<&AppConfig>,
) -> AppResult<Value> {
    let mut app = match app_override {
        Some(app) => app.clone(),
        None => common::lock(state.app_config(), "app_config")?.clone(),
    };
    if let Some(dns) = dns_override {
        app.dns = dns.clone();
    }
    if let Some(tolerance) = tolerance_override {
        app.url_test.tolerance_ms = tolerance;
    }
    let (source_profile_id, selections) = policy_selection::saved_selections(state, base)?;
    let (app, config) =
        crate::configuration::local_edits::resolve(&app, source_profile_id.as_deref(), &config)?;
    let inputs = crate::configuration::composition::Inputs {
        source_profile_id,
        app,
        supports_tolerance: url_test::supports_tolerance(state),
        selections,
    };
    let candidate = crate::configuration::composition::finalize(base, config, &inputs)?;
    state.configuration().record(candidate.report.clone());
    Ok(candidate.config)
}

#[cfg(test)]
use crate::configuration::dns::resolve_dns_detours;

use crate::configuration::rules::{common_tag, eligible_profiles};

pub fn status(state: &AppState) -> AppResult<CommonRuleInjectionStatus> {
    let (scoped, _) = crate::configuration::preferences::current(state)?;
    let enabled = scoped.routing.inject_common_rules;
    let eligible_count = {
        let profiles = common::lock(state.rule_sets(), "rule_set")?;
        eligible_profiles(&profiles).count()
    };
    let mode = {
        let profiles = common::lock(state.proxy_configs(), "proxy_config")?;
        profiles
            .iter()
            .find(|profile| profile.active)
            .and_then(|profile| profile.content.as_ref())
            .and_then(proxy_mode::detect_route_mode)
            .map(|detected| match detected.mode {
                GuiProxyMode::Global => "global".to_string(),
                GuiProxyMode::Rule => "rule".to_string(),
                GuiProxyMode::Direct => "direct".to_string(),
            })
    };
    let Some(mode) = mode else {
        return Ok(CommonRuleInjectionStatus {
            enabled,
            effective: false,
            mode: None,
            eligible_count,
            injected_count: 0,
            reason: Some("当前没有活动配置".to_string()),
        });
    };
    let inherited = active_content(state)?
        .is_some_and(|base| base.pointer("/route/rules").is_some())
        && !scoped.overrides.rules;
    let effective = enabled && !inherited && mode == "rule" && eligible_count > 0;
    let reason = if inherited {
        Some("使用配置中的规则；未开启客户端规则追加".to_string())
    } else if !enabled {
        Some("公共规则注入已关闭".to_string())
    } else if mode != "rule" {
        Some("等待活动配置切换到规则模式".to_string())
    } else if eligible_count == 0 {
        Some("尚未启用公共规则".to_string())
    } else {
        None
    };
    Ok(CommonRuleInjectionStatus {
        enabled,
        effective,
        mode: Some(mode),
        eligible_count,
        injected_count: if effective { eligible_count } else { 0 },
        reason,
    })
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
    let (id, _) = crate::configuration::local_edits::active(state.inner())?;
    let changes = std::collections::BTreeMap::from([(
        "routing.injectCommonRules".to_owned(),
        serde_json::json!(enabled),
    )]);
    let next = crate::configuration::local_edits::candidate(&previous, &id, changes, &[])?;
    crate::commands::app_config::apply_candidate(app_handle.clone(), state.clone(), previous, next)
        .await?;
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
        .map(|base| {
            compose_effective_with(state.inner(), base, inject_enabled, &previous, None, None)
        })
        .transpose()?;
    let next_effective = base
        .as_ref()
        .map(|base| compose_effective_with(state.inner(), base, inject_enabled, &next, None, None))
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

fn compose_rules_for(
    base: &Value,
    enabled: bool,
    profiles: &[RuleSetProfile],
    force: bool,
) -> AppResult<Value> {
    if !force && base.pointer("/route/rules").is_some() {
        crate::configuration::route_integrity::validate(base)?;
        return Ok(base.clone());
    }
    Ok(compose_with(base, enabled, profiles)?.config)
}

fn compose_with(
    base: &Value,
    enabled: bool,
    profiles: &[RuleSetProfile],
) -> AppResult<crate::configuration::rules::ComposeResult> {
    crate::configuration::rules::compose_with(base, enabled, profiles, verify_artifact)
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
    crate::services::config_apply::apply(config, core_config::ipc_options_from_app_config(&core))
        .await
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
    fn explicit_dns_override_replaces_profile_dns() {
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
        app_config.profile_edits.insert(
            "dns-test".into(),
            std::collections::BTreeMap::from([("dns".into(), json!(app_config.dns))]),
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

        let effective = compose_effective_config_for(&state, &base, Some("dns-test")).unwrap();
        assert_eq!(effective["runtime"]["dns"]["default_server"], "system");
        assert_eq!(effective["runtime"]["dns"]["answer"]["type"], "fake_ip");
        assert!(effective.pointer("/runtime/tun").is_none());
        assert!(effective["runtime"]["dns"]["servers"]
            .get("stale")
            .is_none());
    }

    fn dns_with_detour(detour: &str) -> Value {
        json!({
            "servers": {
                "cloudflare": {
                    "type": "doh",
                    "host": "cloudflare-dns.com",
                    "port": 443,
                    "path": "/dns-query",
                    "bootstrap": ["1.1.1.1"],
                    "detour": detour
                }
            },
            "default_server": "cloudflare"
        })
    }

    #[test]
    fn dns_detour_follows_each_target_profiles_route_final() {
        for outbound in ["节点选择", "Others"] {
            let config = json!({
                "outbound_groups": [{ "tag": outbound, "type": "selector", "outbounds": [] }],
                "route": { "final": { "type": "route", "outbound": outbound } }
            });
            let mut dns = dns_with_detour(CLIENT_DNS_DETOUR_ROUTE_FINAL);

            resolve_dns_detours(&config, &mut dns).unwrap();

            assert_eq!(dns["servers"]["cloudflare"]["detour"], outbound);
            assert_ne!(
                dns["servers"]["cloudflare"]["detour"],
                CLIENT_DNS_DETOUR_ROUTE_FINAL
            );
        }
    }

    #[test]
    fn dns_detour_following_direct_route_final_is_omitted() {
        let config = json!({ "route": { "final": { "type": "direct" } } });
        let mut dns = dns_with_detour(CLIENT_DNS_DETOUR_ROUTE_FINAL);

        resolve_dns_detours(&config, &mut dns).unwrap();

        assert!(dns["servers"]["cloudflare"].get("detour").is_none());
    }

    #[test]
    fn fixed_dns_detour_must_exist_in_target_profile() {
        let config = json!({
            "outbounds": [{ "tag": "Proxy", "protocol": { "type": "direct" } }],
            "route": { "final": { "type": "route", "outbound": "Proxy" } }
        });
        let mut valid = dns_with_detour("Proxy");
        resolve_dns_detours(&config, &mut valid).unwrap();
        assert_eq!(valid["servers"]["cloudflare"]["detour"], "Proxy");

        let mut stale = dns_with_detour("节点选择");
        let error = resolve_dns_detours(&config, &mut stale).unwrap_err();
        assert_eq!(error.code, "invalid_argument");
        assert!(error.message.contains("undefined detour `节点选择`"));
    }

    #[test]
    fn dns_detour_cannot_follow_rejecting_or_missing_route_final() {
        for config in [
            json!({ "route": { "final": { "type": "reject" } } }),
            json!({ "route": { "rules": [] } }),
        ] {
            let mut dns = dns_with_detour(CLIENT_DNS_DETOUR_ROUTE_FINAL);
            let error = resolve_dns_detours(&config, &mut dns).unwrap_err();
            assert_eq!(error.code, "invalid_argument");
            assert!(error.message.contains("route.final"));
        }
    }

    #[test]
    fn imported_app_config_is_composed_against_the_active_profile_before_persisting() {
        let base = json!({
            "outbounds": [{ "tag": "Proxy", "protocol": { "type": "direct" } }],
            "route": { "final": { "type": "route", "outbound": "Proxy" } }
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
            Vec::new(),
            Vec::new(),
        );
        let mut candidate = AppConfig::default();
        candidate.dns.config = Some(
            serde_json::from_value(dns_with_detour("Missing")).expect("valid client DNS shape"),
        );

        let error = validate_app_config_candidate(&state, &candidate).unwrap_err();

        assert_eq!(error.code, "invalid_argument");
        assert!(error.message.contains("undefined detour `Missing`"));
    }

    #[test]
    fn built_in_dns_detours_do_not_require_profile_outbounds() {
        let config = json!({ "route": { "final": { "type": "direct" } } });

        for target in ["direct", "block"] {
            let mut dns = dns_with_detour(target);
            resolve_dns_detours(&config, &mut dns).unwrap();
            assert_eq!(dns["servers"]["cloudflare"]["detour"], target);
        }
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
        let mut app = AppConfig::default();
        app.overrides.rules = true;
        crate::configuration::local_edits::migrate_legacy(&mut app, Some("active"));
        let state = AppState::with_domain_data(
            app,
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
    fn undefined_rule_set_references_reject_the_candidate_without_changing_policy() {
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
        assert!(compose_with(&base, false, &[]).is_err());
        assert_eq!(base["route"]["rules"].as_array().unwrap().len(), 3);
    }
}
