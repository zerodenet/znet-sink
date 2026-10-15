use super::*;
use crate::models::{
    app_config::AppConfig, proxy_config::ProxyConfigProfile, rule_references::Blocker,
    rule_set::RuleSetProfile, subscription::SubscriptionProfile,
};
use std::collections::BTreeMap;

pub(crate) struct Prepared {
    pub app: AppConfig,
    pub references: Vec<Reference>,
    pub blockers: Vec<Blocker>,
    pub from_tag: String,
    pub to_tag: Option<String>,
    pub common_binding_removed: bool,
}
fn blocker(profile: Option<&ProxyConfigProfile>, code: &'static str) -> Blocker {
    let reason = match code {
        "managed_source" => "来源由插件或订阅托管；此入口不能检查或迁移受保护材料",
        "source_uninspectable" => "来源没有普通可读正文；无法证明删除不会留下引用",
        "unsupported_source" => "此来源或规则资源不支持普通规则集迁移",
        "source_schema_invalid" => "来源的原生条件结构无法安全检查；原材料保留",
        "metadata_invalid" => "已有覆盖结构无法安全检查；请先处理原覆盖",
        "metadata_source_changed" => "来源已变化；原覆盖不能自动重新绑定",
        "generated_tag_collision" => "来源声明与客户端生成规则集标签冲突；不能按名称或路径推断归属",
        "cross_profile_reference" => "另一配置仍引用此规则集；本次不能扩大确认范围",
        "global_dns_scope" => "全局 DNS 引用影响多份配置；不能证明替代规则集在全部配置可用",
        "legacy_dns_scope" => "另一配置保留旧 DNS 引用；需在原范围明确处理",
        "replacement_unavailable" => {
            "替代必须是现有、已启用且拥有有效公共绑定和产物的普通 GUI 规则集"
        }
        "dns_incompatible" => "替代规则集不满足 DNS 查询前分流的格式要求",
        _ => "引用数量或材料预算超限；不能安全确认完整范围",
    };
    Blocker {
        profile_id: profile.map(|p| p.id.clone()),
        profile_name: profile.map(|p| p.name.clone()),
        reason_code: code,
        reason,
    }
}
fn scoped(
    mut refs: Vec<Reference>,
    profile: Option<&ProxyConfigProfile>,
    prefix: &str,
    kind: Option<&'static str>,
) -> Vec<Reference> {
    for value in &mut refs {
        value.profile_id = profile.map(|p| p.id.clone());
        value.field_path = format!("{prefix}{}", value.field_path);
        if let Some(kind) = kind {
            value.kind = kind;
        }
    }
    refs
}
fn wrapper_dns(value: &Value) -> AppResult<Vec<Reference>> {
    budget(value)?;
    let object = value.as_object().ok_or_else(invalid)?;
    opaque_header(value)?;
    let mut visited = 0;
    for (key, value) in object {
        if key != "config" {
            reject_opaque(value, 0, &mut visited)?;
        }
    }
    match object.get("config").filter(|v| !v.is_null()) {
        Some(config) => scan_dns(config),
        None => Ok(Vec::new()),
    }
}
fn extend_refs(result: &mut Prepared, refs: Vec<Reference>) -> bool {
    let remaining = MAX_REFERENCES.saturating_sub(result.references.len());
    let overflow = refs.len() > remaining;
    result.references.extend(refs.into_iter().take(remaining));
    if overflow {
        result
            .blockers
            .push(blocker(None, "reference_budget_exceeded"));
    }
    !overflow
}
fn rewrite_wrapper(value: &Value, changes: &[Replacement]) -> AppResult<Value> {
    wrapper_dns(value)?;
    let mut result = value.clone();
    if let Some(config) = value.get("config").filter(|v| !v.is_null()) {
        result["config"] = rewrite_dns(config, changes)?;
    }
    Ok(result)
}

/// Pure fixed graph preparation. No ownership claim is inferred from file paths,
/// source names or matching arbitrary native tags. Any blocker yields the exact
/// original AppConfig, never a partly rewritten candidate.
pub(crate) fn prepare(
    app: &AppConfig,
    profiles: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
    rules: &[RuleSetProfile],
    current_profile_id: Option<&str>,
    id: &str,
    target_id: Option<&str>,
) -> AppResult<Prepared> {
    let removed = rules
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| AppError::not_found("rule_set", id))?;
    let mut result = Prepared {
        app: app.clone(),
        references: Vec::new(),
        blockers: Vec::new(),
        from_tag: super::super::rules::common_tag(id),
        to_tag: target_id.map(super::super::rules::common_tag),
        common_binding_removed: removed.common_binding.as_ref().is_some_and(|v| v.enabled),
    };
    let managed = |rule: &RuleSetProfile| {
        rule.managed_by_subscription_id.is_some()
            || crate::services::rule_set::is_managed_subscription_rule_set_id(&rule.id)
    };
    if removed.built_in || managed(removed) {
        result.blockers.push(blocker(None, "unsupported_source"));
    }
    let mut tags = BTreeSet::new();
    if rules
        .iter()
        .any(|r| !tags.insert(super::super::rules::common_tag(&r.id)))
    {
        result
            .blockers
            .push(blocker(None, "generated_tag_collision"));
    }
    if let Some(target_id) = target_id {
        let target = rules.iter().find(|r| r.id == target_id);
        if target_id == id
            || target.is_none_or(|r| {
                r.built_in
                    || managed(r)
                    || !super::super::rules::eligible_profiles(std::slice::from_ref(r))
                        .any(|_| true)
            })
        {
            result
                .blockers
                .push(blocker(None, "replacement_unavailable"));
        }
    }
    if profiles.len() > 256 || rules.len() > MAX_REFERENCES || subscriptions.len() > MAX_REFERENCES
    {
        result
            .blockers
            .push(blocker(None, "reference_budget_exceeded"));
        return Ok(result);
    }
    let mut existing = BTreeMap::new();
    let mut source_bytes = 0usize;
    for profile in profiles {
        // These checks precede any source-body access, including serialization.
        if profile.managed_source.is_some()
            || subscriptions.iter().any(|s| {
                s.target_proxy_config_id.as_deref() == Some(&profile.id)
                    && s.managed_source.is_some()
            })
        {
            result
                .blockers
                .push(blocker(Some(profile), "managed_source"));
            continue;
        }
        if profile.content.is_none() {
            result
                .blockers
                .push(blocker(Some(profile), "source_uninspectable"));
            continue;
        }
        if profile.kernel != "zero" {
            result
                .blockers
                .push(blocker(Some(profile), "unsupported_source"));
            continue;
        }
        let source = profile.content.as_ref().expect("admitted body");
        source_bytes = match encoded_size(source)
            .ok()
            .and_then(|n| source_bytes.checked_add(n))
            .filter(|n| *n <= MAX_BYTES)
        {
            Some(n) => n,
            None => {
                result
                    .blockers
                    .push(blocker(Some(profile), "reference_budget_exceeded"));
                continue;
            }
        };
        let source_refs = match scan(source) {
            Ok(v) => v,
            Err(_) => {
                result
                    .blockers
                    .push(blocker(Some(profile), "source_schema_invalid"));
                continue;
            }
        };
        if source
            .pointer("/route/rule_sets")
            .and_then(Value::as_array)
            .is_some_and(|rows| {
                rows.iter().any(|r| {
                    r.get("tag")
                        .and_then(Value::as_str)
                        .is_some_and(|v| v.starts_with("gui-common-"))
                })
            })
        {
            result
                .blockers
                .push(blocker(Some(profile), "generated_tag_collision"));
        }
        let edits = app.profile_edits.get(&profile.id);
        let prior = match edits.and_then(|e| e.get("ruleSetReferences")) {
            None => Vec::new(),
            Some(value) => match parse_metadata(value) {
                Ok(value)
                    if super::super::route_rules::source_digest(source)? == value.source_digest =>
                {
                    value.replacements
                }
                Ok(_) => {
                    result
                        .blockers
                        .push(blocker(Some(profile), "metadata_source_changed"));
                    continue;
                }
                Err(_) => {
                    result
                        .blockers
                        .push(blocker(Some(profile), "metadata_invalid"));
                    continue;
                }
            },
        };
        let mut refs = source_refs;
        // Inventory uses the existing projection; old original tags are retained
        // by metadata for a future explicit restore of the routeRules override.
        for reference in &mut refs {
            if let Some(change) = prior.iter().find(|r| r.from_tag == reference.tag) {
                reference.tag = change.to_tag.clone();
            }
        }
        if let Some(rows) = edits.and_then(|e| e.get("routeRules")) {
            if rows.get("sourceDigest").and_then(Value::as_str)
                != Some(super::super::route_rules::source_digest(source)?.as_str())
            {
                result
                    .blockers
                    .push(blocker(Some(profile), "metadata_source_changed"));
            } else {
                match rewrite_route_rows(rows, &prior).and_then(|v| scan_route_rows(&v)) {
                    Ok(found) => {
                        refs.extend(scoped(found, None, "profileEdits.routeRules.", None));
                    }
                    Err(_) => result
                        .blockers
                        .push(blocker(Some(profile), "metadata_invalid")),
                }
                // Consumption order is routeRules first, reference projection
                // second. Typed inventory alone must not certify an overlay
                // that the existing routeRules gate cannot actually consume.
                // That gate denies rule/GeoIP dependencies before native IO.
                if super::super::route_rules::apply(source, rows).is_err() {
                    result
                        .blockers
                        .push(blocker(Some(profile), "metadata_invalid"));
                }
            }
        }
        let selected: Vec<_> = refs
            .into_iter()
            .filter(|r| r.tag == result.from_tag)
            .collect();
        if !selected.is_empty() && current_profile_id != Some(profile.id.as_str()) {
            result
                .blockers
                .push(blocker(Some(profile), "cross_profile_reference"));
        }
        if !extend_refs(&mut result, scoped(selected, Some(profile), "", None)) {
            return Ok(result);
        }
        existing.insert(profile.id.clone(), prior);
        if let Some(dns) = edits.and_then(|e| e.get("dns")) {
            match wrapper_dns(dns) {
                Ok(refs) => {
                    let selected: Vec<_> = refs
                        .into_iter()
                        .filter(|r| r.tag == result.from_tag)
                        .collect();
                    if !selected.is_empty() && current_profile_id != Some(profile.id.as_str()) {
                        result
                            .blockers
                            .push(blocker(Some(profile), "legacy_dns_scope"));
                    }
                    if !extend_refs(
                        &mut result,
                        scoped(
                            selected,
                            Some(profile),
                            "profileEdits.dns.config.",
                            Some("legacy_dns"),
                        ),
                    ) {
                        return Ok(result);
                    }
                }
                Err(_) => result
                    .blockers
                    .push(blocker(Some(profile), "metadata_invalid")),
            }
        }
    }
    // Orphan profile overrides must not disappear from the inventory.
    if app
        .profile_edits
        .keys()
        .any(|id| !profiles.iter().any(|p| &p.id == id))
    {
        result.blockers.push(blocker(None, "source_uninspectable"));
    }
    if app
        .client_edits
        .as_ref()
        .is_some_and(|e| e.contains_key("ruleSetReferences") || e.contains_key("routeRules"))
    {
        result.blockers.push(blocker(None, "metadata_invalid"));
    }
    let legacy_dns = serde_json::to_value(&app.dns).map_err(|_| invalid())?;
    for (dns, path, kind) in [
        (Some(&legacy_dns), "dns.config.", "legacy_dns"),
        (
            app.client_edits.as_ref().and_then(|e| e.get("dns")),
            "clientEdits.dns.config.",
            "global_dns",
        ),
    ] {
        if let Some(dns) = dns {
            match wrapper_dns(dns) {
                Ok(refs) => {
                    let selected: Vec<_> = refs
                        .into_iter()
                        .filter(|r| r.tag == result.from_tag)
                        .collect();
                    if !selected.is_empty()
                        && (profiles.len() != 1
                            || profiles.first().map(|p| p.id.as_str()) != current_profile_id)
                    {
                        result.blockers.push(blocker(None, "global_dns_scope"));
                    }
                    if !extend_refs(&mut result, scoped(selected, None, path, Some(kind))) {
                        return Ok(result);
                    }
                }
                Err(_) => result.blockers.push(blocker(None, "metadata_invalid")),
            }
        }
    }
    if result.references.len() > MAX_REFERENCES {
        result
            .blockers
            .push(blocker(None, "reference_budget_exceeded"));
    }
    if !result.references.is_empty() {
        // The common target is only defined when actual common composition is
        // enabled in rule mode. Do not silently toggle this user's preference.
        let mut effective_settings = app.clone();
        super::super::local_edits::migrate_legacy(&mut effective_settings, current_profile_id);
        let common_enabled = match effective_settings
            .client_edits
            .as_ref()
            .and_then(|e| e.get("routing.injectCommonRules"))
        {
            None => effective_settings.routing.inject_common_rules,
            Some(value) => match value.as_bool() {
                Some(v) => v,
                None => {
                    result.blockers.push(blocker(None, "metadata_invalid"));
                    false
                }
            },
        };
        let rule_mode = profiles
            .iter()
            .find(|p| Some(p.id.as_str()) == current_profile_id)
            .and_then(|p| p.content.as_ref())
            .and_then(crate::services::proxy_mode::detect_route_mode)
            .is_some_and(|m| matches!(m.mode, crate::models::gui_core::GuiProxyMode::Rule));
        if !common_enabled || !rule_mode {
            result
                .blockers
                .push(blocker(None, "replacement_unavailable"));
        }
    }
    if !result.blockers.is_empty() || target_id.is_none() || result.references.is_empty() {
        return Ok(result);
    }
    let changes = [Replacement {
        from_tag: result.from_tag.clone(),
        to_tag: result.to_tag.clone().ok_or_else(invalid)?,
    }];
    let mut next = app.clone();
    for profile in profiles {
        if current_profile_id != Some(profile.id.as_str()) {
            continue;
        }
        if result
            .references
            .iter()
            .any(|r| r.profile_id.as_deref() == Some(&profile.id) && r.kind != "legacy_dns")
        {
            let mut mappings = existing.remove(&profile.id).unwrap_or_default();
            for value in &mut mappings {
                if value.to_tag == changes[0].from_tag {
                    value.to_tag = changes[0].to_tag.clone();
                }
            }
            if !mappings.iter().any(|r| r.from_tag == changes[0].from_tag) {
                mappings.push(changes[0].clone());
            }
            let value = metadata(profile.content.as_ref().ok_or_else(invalid)?, &mappings)?;
            next.profile_edits
                .entry(profile.id.clone())
                .or_default()
                .insert("ruleSetReferences".into(), value);
        }
        if let Some(dns) = next
            .profile_edits
            .get_mut(&profile.id)
            .and_then(|e| e.get_mut("dns"))
        {
            *dns = rewrite_wrapper(dns, &changes)?;
        }
    }
    next.dns =
        serde_json::from_value(rewrite_wrapper(&legacy_dns, &changes)?).map_err(|_| invalid())?;
    if let Some(dns) = next.client_edits.as_mut().and_then(|e| e.get_mut("dns")) {
        *dns = rewrite_wrapper(dns, &changes)?;
    }
    // No source, SQL relationship, target binding, artifact or runtime mutation.
    result.app = next;
    Ok(result)
}
