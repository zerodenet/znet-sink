//! Fixed, metadata-only uninstall inventory. No path is opened and no state is
//! mutated. An ID prefix is a collision detector, never ownership authority.
use crate::{
    configuration::{route_rules, rule_references, rules as native_rules},
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig,
        proxy_config::ProxyConfigProfile,
        rule_set::RuleSetProfile,
        subscription::{ManagedSubscriptionSource, SubscriptionProfile},
    },
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_RESOURCES: usize = 128;
const MAX_GRAPH: usize = 4096;
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Resource {
    pub kind: &'static str,
    pub id: String,
    pub profile_id: Option<String>,
    pub field_path: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Blocker {
    pub kind: &'static str,
    pub id: String,
    pub reason_code: &'static str,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Replacement {
    pub id: String,
    pub name: String,
    pub eligible: bool,
    pub reason_code: Option<&'static str>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResourcePlan {
    pub profile_ids: Vec<String>,
    pub subscription_ids: Vec<String>,
    pub rule_set_ids: Vec<String>,
    pub active_profile_id: Option<String>,
    pub replacements: Vec<Replacement>,
    pub shared_resources: Vec<Resource>,
    pub blockers: Vec<Blocker>,
}
impl ResourcePlan {
    fn block(&mut self, kind: &'static str, id: &str, reason_code: &'static str) {
        if self.blockers.len() < MAX_RESOURCES
            && !self
                .blockers
                .iter()
                .any(|b| b.kind == kind && b.id == id && b.reason_code == reason_code)
        {
            self.blockers.push(Blocker {
                kind,
                id: id.into(),
                reason_code,
            });
        }
    }
    fn shared(&mut self, kind: &'static str, id: &str, profile: Option<&str>, path: Option<&str>) {
        if self.shared_resources.len() >= MAX_RESOURCES {
            self.block("graph", "", "resource_budget_exceeded");
            return;
        }
        self.shared_resources.push(Resource {
            kind,
            id: id.into(),
            profile_id: profile.map(str::to_owned),
            field_path: path.map(str::to_owned),
        });
        self.block(kind, id, "shared_resource");
    }
}
fn valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
fn namespace(owner: &ManagedSubscriptionSource) -> Option<(String, String)> {
    if !valid(&owner.plugin_id)
        || !valid(&owner.provider_id)
        || !valid(&owner.remote_subscription_id)
        || [
            &owner.plugin_id,
            &owner.provider_id,
            &owner.remote_subscription_id,
        ]
        .iter()
        .any(|v| v.contains('\0'))
    {
        return None;
    }
    let digest = Sha256::digest(
        format!(
            "{}\0{}\0{}",
            owner.plugin_id, owner.provider_id, owner.remote_subscription_id
        )
        .as_bytes(),
    );
    let suffix = digest[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Some((
        format!("managed-{suffix}"),
        format!("managed-config-{suffix}"),
    ))
}
fn prefix(subscription: &str) -> String {
    format!("subscription-rule-{subscription}-")
}
fn managed_rule_id(subscription: &str, tag: &str) -> String {
    let digest = Sha256::digest(tag.as_bytes());
    format!(
        "{}{}",
        prefix(subscription),
        digest[..12]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn budget(value: &impl Serialize, used: &mut usize) -> AppResult<()> {
    struct Counter<'a>(&'a mut usize);
    impl std::io::Write for Counter<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            *self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_BYTES)
                .ok_or_else(|| std::io::Error::other("budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(used), value).map_err(|_| invalid())
}
fn invalid() -> AppError {
    AppError::invalid_argument("插件卸载资源无法完整核对；原材料保留")
}
fn opaque(value: &Value, depth: usize, visited: &mut usize) -> AppResult<()> {
    *visited = visited.checked_add(1).ok_or_else(invalid)?;
    if depth > 64 || *visited > 100_000 {
        return Err(invalid());
    }
    match value {
        Value::Object(v) => {
            if v.get("type").and_then(Value::as_str) == Some("rule_set")
                || v.contains_key("rule_set")
                || v.get("tag").is_some_and(Value::is_string)
            {
                return Err(invalid());
            }
            for child in v.values() {
                opaque(child, depth + 1, visited)?;
            }
        }
        Value::Array(v) => {
            for child in v {
                opaque(child, depth + 1, visited)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn dns_refs(value: &Value) -> AppResult<Vec<crate::models::rule_references::Reference>> {
    let object = value.as_object().ok_or_else(invalid)?;
    let mut visited = 0;
    for (key, v) in object {
        if key != "config" {
            opaque(v, 0, &mut visited)?;
        }
    }
    match object.get("config").filter(|v| !v.is_null()) {
        Some(config) => rule_references::scan_dns(config),
        None => Ok(vec![]),
    }
}
fn associated_rule<'a>(rules: &'a [&RuleSetProfile], tag: &str) -> Option<&'a RuleSetProfile> {
    rules.iter().copied().find(|r| {
        native_rules::common_tag(&r.id) == tag
            || r.managed_by_subscription_id
                .as_ref()
                .is_some_and(|s| managed_rule_id(s, tag) == r.id)
    })
}
fn inspect_refs(
    plan: &mut ResourcePlan,
    refs: Vec<crate::models::rule_references::Reference>,
    rules: &[&RuleSetProfile],
    profile: Option<&str>,
    prefix: &str,
    source: Option<&Value>,
) {
    for reference in refs {
        if let Some(rule) = associated_rule(rules, &reference.tag) {
            // Native tags are local to a config. A separately declared definition
            // with a different artifact is not the plugin's rule-set resource.
            let independently_defined = native_rules::common_tag(&rule.id) != reference.tag
                && source
                    .and_then(|s| s.pointer("/route/rule_sets"))
                    .and_then(Value::as_array)
                    .is_some_and(|defs| {
                        defs.iter().any(|d| {
                            d["tag"].as_str() == Some(reference.tag.as_str())
                                && d["path"].as_str().is_some_and(|p| {
                                    rule.artifact.as_ref().is_none_or(|a| a.path != p)
                                })
                        })
                    });
            if !independently_defined {
                plan.shared(
                    "rule_set",
                    &rule.id,
                    profile,
                    Some(&format!("{prefix}{}", reference.field_path)),
                );
            }
        }
    }
}

/// The host supplies component IDs from its verified installation, never from
/// a guest. provider_id is independent of component_id (SDK providers may be
/// HTTPS origins), so membership is deliberately not guessed from that field.
pub(super) fn collect(
    profiles: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
    rules: &[RuleSetProfile],
    app: &AppConfig,
    plugin_id: &str,
    verified_component_ids: &[String],
) -> AppResult<ResourcePlan> {
    if !valid(plugin_id)
        || verified_component_ids.is_empty()
        || verified_component_ids.len() > MAX_RESOURCES
        || verified_component_ids.iter().any(|v| !valid(v))
        || verified_component_ids.iter().collect::<BTreeSet<_>>().len()
            != verified_component_ids.len()
    {
        return Err(invalid());
    }
    let mut plan = ResourcePlan::default();
    if profiles.len() > 256 || subscriptions.len() > MAX_GRAPH || rules.len() > MAX_GRAPH {
        plan.block("graph", "", "resource_budget_exceeded");
        return Ok(plan);
    }
    let mut used = 0;
    if budget(app, &mut used).is_err() {
        plan.block("app_config", "", "resource_budget_exceeded");
        return Ok(plan);
    }
    for (kind, ids) in [
        (
            "profile",
            profiles.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        ),
        (
            "subscription",
            subscriptions.iter().map(|v| v.id.as_str()).collect(),
        ),
        ("rule_set", rules.iter().map(|v| v.id.as_str()).collect()),
    ] {
        let mut seen = BTreeSet::new();
        for id in ids {
            if !valid(id) || !seen.insert(id) {
                plan.block(kind, "", "identity_invalid");
            }
        }
    }
    let mut owned_subs = BTreeSet::new();
    for p in profiles.iter().filter(|p| {
        p.managed_source
            .as_ref()
            .is_some_and(|o| o.plugin_id == plugin_id)
    }) {
        let owner = p.managed_source.as_ref().expect("filtered owner");
        let Some((sub_id, profile_id)) = namespace(owner) else {
            plan.block("profile", &p.id, "ownership_invalid");
            continue;
        };
        if p.id != profile_id {
            plan.block("profile", &p.id, "ownership_invalid");
            continue;
        }
        // Managed does not itself mean protected. Ordinary managed sources are
        // decoded JSON. A path may be provider provenance; never open it. Only
        // after exact ownership is admitted may this host inspect ordinary JSON.
        if p.kernel != "zero" || p.format != "json" {
            plan.block("profile", &p.id, "unsupported_source");
            continue;
        }
        let Some(source) = p.content.as_ref().filter(|v| v.is_object()) else {
            plan.block("profile", &p.id, "source_uninspectable");
            continue;
        };
        if budget(source, &mut used).is_err() || rule_references::scan(source).is_err() {
            plan.block("profile", &p.id, "source_schema_invalid");
            continue;
        }
        if subscriptions.iter().any(|s| {
            s.target_proxy_config_id.as_deref() == Some(&p.id)
                && s.managed_source
                    .as_ref()
                    .is_none_or(|o| !o.same_namespace(owner))
        }) {
            plan.shared("profile", &p.id, None, None);
        }
        owned_subs.insert(sub_id);
        plan.profile_ids.push(p.id.clone());
        if p.active {
            plan.active_profile_id = Some(p.id.clone());
            plan.block("profile", &p.id, "active_profile_requires_switch");
        }
    }
    for s in subscriptions.iter().filter(|s| {
        s.managed_source
            .as_ref()
            .is_some_and(|o| o.plugin_id == plugin_id)
    }) {
        let owner = s.managed_source.as_ref().expect("filtered owner");
        let Some((sub_id, profile_id)) = namespace(owner) else {
            plan.block("subscription", &s.id, "ownership_invalid");
            continue;
        };
        if s.id != sub_id
            || s.target_proxy_config_id
                .as_ref()
                .is_some_and(|id| id != &profile_id)
            || profiles.iter().any(|p| {
                p.id == profile_id
                    && p.managed_source
                        .as_ref()
                        .is_none_or(|o| !o.same_namespace(owner))
            })
        {
            plan.block("subscription", &s.id, "ownership_invalid");
            continue;
        }
        owned_subs.insert(sub_id);
        plan.subscription_ids.push(s.id.clone());
    }
    for r in rules {
        let owner = r.managed_by_subscription_id.as_deref();
        let prefix_owner = owned_subs.iter().find(|s| r.id.starts_with(&prefix(s)));
        if owner.is_some_and(|s| owned_subs.contains(s)) {
            let valid_suffix = owner.is_some_and(|s| {
                r.id.strip_prefix(&prefix(s)).is_some_and(|v| {
                    v.len() == 24
                        && v.bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
            });
            if r.built_in
                || r.common_binding.is_some()
                || prefix_owner.map(String::as_str) != owner
                || !valid_suffix
            {
                plan.block("rule_set", &r.id, "ownership_invalid");
                continue;
            }
            plan.rule_set_ids.push(r.id.clone());
        } else if prefix_owner.is_some() {
            plan.block("rule_set", &r.id, "ownership_collision");
        }
    }
    if plan.profile_ids.len() + plan.subscription_ids.len() + plan.rule_set_ids.len()
        > MAX_RESOURCES
    {
        plan.block("graph", "", "resource_budget_exceeded");
        plan.profile_ids.clear();
        plan.subscription_ids.clear();
        plan.rule_set_ids.clear();
        return Ok(plan);
    }
    if profiles.iter().filter(|p| p.active).count() > 1 {
        plan.block("graph", "", "active_identity_ambiguous");
    }
    let selected: Vec<_> = rules
        .iter()
        .filter(|r| plan.rule_set_ids.contains(&r.id))
        .collect();
    let deleted_profiles: BTreeSet<_> = plan.profile_ids.iter().cloned().collect();
    for p in profiles
        .iter()
        .filter(|p| !deleted_profiles.contains(&p.id))
    {
        let protected = p.managed_source.is_some()
            || subscriptions.iter().any(|s| {
                s.target_proxy_config_id.as_deref() == Some(&p.id) && s.managed_source.is_some()
            });
        let reason = if protected {
            Some("protected_source")
        } else if p.kernel != "zero" || p.format != "json" {
            Some("unsupported_source")
        } else if p.content.is_none() {
            Some("source_uninspectable")
        } else {
            None
        };
        let mut candidate_reason = reason;
        if reason.is_none() {
            let source = p.content.as_ref().expect("admitted source");
            if budget(source, &mut used).is_err() || rule_references::scan(source).is_err() {
                candidate_reason = Some("source_schema_invalid");
            }
        }
        plan.replacements.push(Replacement {
            id: p.id.clone(),
            name: p.name.clone(),
            eligible: candidate_reason.is_none(),
            reason_code: candidate_reason,
        });
        if selected.is_empty() {
            continue;
        }
        if let Some(reason) = candidate_reason {
            plan.block("profile", &p.id, reason);
            continue;
        }
        let source = p.content.as_ref().expect("admitted source");
        inspect_refs(
            &mut plan,
            rule_references::scan(source)?,
            &selected,
            Some(&p.id),
            "",
            Some(source),
        );
        for (i, d) in source
            .pointer("/route/rule_sets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            for r in &selected {
                if d["path"]
                    .as_str()
                    .is_some_and(|path| r.artifact.as_ref().is_some_and(|a| a.path == path))
                {
                    plan.shared(
                        "rule_set",
                        &r.id,
                        Some(&p.id),
                        Some(&format!("route.rule_sets[{i}]")),
                    );
                }
            }
        }
        if let Some(edits) = app.profile_edits.get(&p.id) {
            if let Some(rows) = edits.get("routeRules") {
                match budget(rows, &mut used).and_then(|_| {
                    if rows["sourceDigest"].as_str()
                        != Some(route_rules::source_digest(source)?.as_str())
                    {
                        return Err(invalid());
                    }
                    rule_references::scan_route_rows(rows)
                }) {
                    Ok(refs) => inspect_refs(
                        &mut plan,
                        refs,
                        &selected,
                        Some(&p.id),
                        "profileEdits.routeRules.",
                        Some(source),
                    ),
                    Err(_) => plan.block("profile", &p.id, "metadata_invalid"),
                }
            }
            if let Some(mapping) = edits.get("ruleSetReferences") {
                match budget(mapping, &mut used)
                    .and_then(|_| rule_references::apply(source, source, mapping))
                    .and_then(|v| rule_references::scan(&v))
                {
                    Ok(refs) => inspect_refs(
                        &mut plan,
                        refs,
                        &selected,
                        Some(&p.id),
                        "profileEdits.ruleSetReferences.",
                        Some(source),
                    ),
                    Err(_) => plan.block("profile", &p.id, "metadata_invalid"),
                }
            }
            if let Some(dns) = edits.get("dns") {
                inspect_dns(
                    &mut plan,
                    dns,
                    &selected,
                    Some(&p.id),
                    "profileEdits.dns.config.",
                    &mut used,
                );
            }
            for (key, value) in edits {
                if !matches!(key.as_str(), "routeRules" | "ruleSetReferences" | "dns")
                    && opaque(value, 0, &mut 0).is_err()
                {
                    plan.block("profile", &p.id, "metadata_invalid");
                }
            }
        }
    }
    if !selected.is_empty() {
        for id in app
            .profile_edits
            .keys()
            .filter(|id| !profiles.iter().any(|p| &p.id == *id))
        {
            plan.block("profile", id, "orphan_metadata");
        }
        inspect_dns(
            &mut plan,
            &serde_json::to_value(&app.dns).map_err(|_| invalid())?,
            &selected,
            None,
            "dns.config.",
            &mut used,
        );
        if let Some(edits) = &app.client_edits {
            for (key, value) in edits {
                if key == "dns" {
                    inspect_dns(
                        &mut plan,
                        value,
                        &selected,
                        None,
                        "clientEdits.dns.config.",
                        &mut used,
                    );
                } else if matches!(key.as_str(), "routeRules" | "ruleSetReferences")
                    || opaque(value, 0, &mut 0).is_err()
                {
                    plan.block("app_config", "", "metadata_invalid");
                }
            }
        }
        let deleted_rules: BTreeSet<_> = plan.rule_set_ids.iter().cloned().collect();
        for r in rules.iter().filter(|r| !deleted_rules.contains(&r.id)) {
            for removed in &selected {
                if r.artifact
                    .as_ref()
                    .zip(removed.artifact.as_ref())
                    .is_some_and(|(a, b)| a.path == b.path)
                {
                    plan.shared("rule_set", &removed.id, None, None);
                }
            }
        }
    }
    plan.profile_ids.sort();
    plan.subscription_ids.sort();
    plan.rule_set_ids.sort();
    Ok(plan)
}
fn inspect_dns(
    plan: &mut ResourcePlan,
    value: &Value,
    selected: &[&RuleSetProfile],
    profile: Option<&str>,
    prefix: &str,
    used: &mut usize,
) {
    match budget(value, used).and_then(|_| dns_refs(value)) {
        Ok(refs) => inspect_refs(plan, refs, selected, profile, prefix, None),
        Err(_) => plan.block("app_config", profile.unwrap_or(""), "metadata_invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const SECRET: &str = "canary-token-never-export";
    fn source() -> Value {
        json!({"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],
            "route":{"rules":[],"final":{"type":"direct"}},
            "runtime":{"log":{"level":"info","files":[{"path":SECRET,"max_bytes":u64::MAX}]}}})
    }
    fn ordinary(id: &str) -> ProxyConfigProfile {
        ProxyConfigProfile {
            id: id.into(),
            name: id.into(),
            kernel: "zero".into(),
            format: "json".into(),
            path: None,
            content: Some(source()),
            active: false,
            managed_source: None,
            updated_at_unix_ms: 1,
            capabilities: Default::default(),
        }
    }
    fn owned(provider: &str, remote: &str) -> (ProxyConfigProfile, SubscriptionProfile) {
        let owner = ManagedSubscriptionSource {
            plugin_id: "plugin-a".into(),
            provider_id: provider.into(),
            remote_subscription_id: remote.into(),
            revision: None,
            source_name: None,
        };
        let (sub, config) = namespace(&owner).unwrap();
        let mut p = ordinary(&config);
        p.managed_source = Some(owner.clone());
        let s = serde_json::from_value(json!({"id":sub,"name":"owned","url":SECRET,
            "enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":config,
            "managedSource":owner,"updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null}))
        .unwrap();
        (p, s)
    }
    fn rules(s: &SubscriptionProfile) -> RuleSetProfile {
        RuleSetProfile {
            id: managed_rule_id(&s.id, "private-tag"),
            name: "rule".into(),
            enabled: true,
            built_in: false,
            provenance: None,
            managed_by_subscription_id: Some(s.id.clone()),
            common_binding: None,
            semantic_ir: json!({"version":1,"rules":[]}),
            source: None,
            source_state: Default::default(),
            artifact: None,
            updated_at_unix_ms: 1,
            last_sync_at_unix_ms: None,
            last_error: None,
        }
    }
    fn collect_test(
        p: &[ProxyConfigProfile],
        s: &[SubscriptionProfile],
        r: &[RuleSetProfile],
        app: &AppConfig,
    ) -> ResourcePlan {
        collect(p, s, r, app, "plugin-a", &["identity".into()]).unwrap()
    }
    fn has(plan: &ResourcePlan, reason: &str) -> bool {
        plan.blockers.iter().any(|b| b.reason_code == reason)
    }
    fn rule(tag: &str) -> Value {
        json!({"condition":{"type":"rule_set","tag":tag},"action":{"type":"direct"},"mode":"flow"})
    }
    fn dns(tag: &str) -> Value {
        json!({"servers":{"system":{"type":"system"}},"default_server":"system",
        "dispatch":[{"condition":{"type":"and","items":[{"type":"rule_set","tag":tag}]},"server":"system"}]})
    }

    #[test]
    fn exact_namespaces_select_all_providers_without_component_provider_guessing() {
        let (p, s) = owned("https://provider.example", "one");
        let (q, t) = owned("different-provider", "two");
        let rs = vec![rules(&s), rules(&t)];
        let plan = collect_test(
            &[p.clone(), q.clone()],
            &[s.clone(), t.clone()],
            &rs,
            &AppConfig::default(),
        );
        assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
        assert_eq!(plan.profile_ids.len(), 2);
        assert_eq!(plan.subscription_ids.len(), 2);
        assert_eq!(plan.rule_set_ids.len(), 2);
        assert!(plan.profile_ids.contains(&p.id));
        assert!(plan.profile_ids.contains(&q.id));
        assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
    }
    #[test]
    fn prefix_collisions_missing_foreign_and_builtin_ownership_never_authorize_delete() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        for variant in 0..4 {
            let mut bad = r.clone();
            match variant {
                0 => bad.managed_by_subscription_id = None,
                1 => bad.managed_by_subscription_id = Some("foreign".into()),
                2 => bad.built_in = true,
                _ => bad.id = format!("{}not-a-valid-hash", prefix(&s.id)),
            }
            let plan = collect_test(
                std::slice::from_ref(&p),
                std::slice::from_ref(&s),
                &[bad],
                &AppConfig::default(),
            );
            assert!(plan.rule_set_ids.is_empty());
            assert!(!plan.blockers.is_empty());
        }
    }
    #[test]
    fn corrupt_namespace_and_foreign_subscription_links_block_before_execution() {
        let (p, s) = owned("provider", "one");
        let mut foreign = s.clone();
        foreign.id = "manual".into();
        foreign.managed_source = None;
        let plan = collect_test(
            std::slice::from_ref(&p),
            &[s.clone(), foreign],
            &[],
            &AppConfig::default(),
        );
        assert!(has(&plan, "shared_resource"));
        let mut corrupt = p;
        corrupt.id = "ordinary".into();
        let plan = collect_test(&[corrupt], &[s], &[], &AppConfig::default());
        assert!(has(&plan, "ownership_invalid"));
        assert!(plan.profile_ids.is_empty());
    }
    #[test]
    fn missing_or_detached_original_owned_relations_are_safe_fixed_metadata_targets() {
        let (p, mut s) = owned("provider", "one");
        s.target_proxy_config_id = None;
        assert!(
            collect_test(&[], &[s.clone()], &[rules(&s)], &AppConfig::default())
                .blockers
                .is_empty()
        );
        let plan = collect_test(&[p], &[], &[rules(&s)], &AppConfig::default());
        assert_eq!(plan.rule_set_ids.len(), 1);
        assert!(plan.subscription_ids.is_empty());
    }
    #[test]
    fn active_is_blocked_without_promoting_first_replacement_and_protected_is_not_eligible() {
        let (mut p, s) = owned("provider", "one");
        p.active = true;
        let mut protected = ordinary("protected");
        protected.managed_source = p.managed_source.clone();
        // Invalid protected namespace is still not a replacement authority.
        protected.managed_source.as_mut().unwrap().plugin_id = "other-plugin".into();
        protected.content = Some(json!({"secret":SECRET}));
        let plan = collect_test(
            &[p.clone(), ordinary("first"), protected],
            &[s],
            &[],
            &AppConfig::default(),
        );
        assert!(has(&plan, "active_profile_requires_switch"));
        assert_eq!(plan.active_profile_id, Some(p.id));
        assert_eq!(plan.replacements.len(), 2);
        assert!(plan.replacements[0].eligible);
        assert!(!plan.replacements[1].eligible);
        assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
    }
    #[test]
    fn remaining_protected_or_unreadable_material_is_not_an_empty_reference_set() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        for variant in 0..3 {
            let mut other = ordinary("other");
            match variant {
                0 => other.content = None,
                1 => other.kernel = "future".into(),
                _ => {
                    other.managed_source = p.managed_source.clone();
                    other.managed_source.as_mut().unwrap().plugin_id = "other-plugin".into();
                    other.content = Some(json!({"raw_secret":SECRET}));
                }
            }
            let plan = collect_test(
                &[p.clone(), other],
                std::slice::from_ref(&s),
                std::slice::from_ref(&r),
                &AppConfig::default(),
            );
            assert!(plan.blockers.iter().any(|b| b.id == "other"));
            assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
        }
    }
    #[test]
    fn source_route_bypass_and_nested_dns_refs_are_each_inventoried_without_io() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        let tag = native_rules::common_tag(&r.id);
        for variant in 0..3 {
            let mut other = ordinary("other");
            let v = other.content.as_mut().unwrap();
            match variant {
                0 => v["route"]["rules"] = json!([rule(&tag)]),
                1 => {
                    v["route"]["bypass"] =
                        json!([{"type":"or","items":[{"type":"rule_set","tag":tag}]}])
                }
                _ => v["runtime"]["dns"] = dns(&tag),
            }
            let before = other.content.clone();
            let plan = collect_test(
                &[p.clone(), other.clone()],
                std::slice::from_ref(&s),
                std::slice::from_ref(&r),
                &AppConfig::default(),
            );
            assert!(has(&plan, "shared_resource"), "{:?}", plan.blockers);
            assert!(plan
                .shared_resources
                .iter()
                .any(|x| x.profile_id.as_deref() == Some("other")));
            assert_eq!(other.content, before);
        }
    }
    #[test]
    fn disabled_overlay_rows_and_source_bound_reference_mapping_are_not_omitted() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        let tag = native_rules::common_tag(&r.id);
        let mut other = ordinary("other");
        let mut app = AppConfig::default();
        let src = other.content.as_ref().unwrap();
        app.profile_edits.entry("other".into()).or_default().insert(
            "routeRules".into(),
            json!({
            "version":1,"sourceDigest":route_rules::source_digest(src).unwrap(),
            "rows":[{"id":"disabled","enabled":false,"rule":rule(&tag)}]}),
        );
        let plan = collect_test(
            &[p.clone(), other.clone()],
            std::slice::from_ref(&s),
            std::slice::from_ref(&r),
            &app,
        );
        assert!(plan.shared_resources.iter().any(|v| v
            .field_path
            .as_deref()
            .unwrap_or("")
            .contains("routeRules")));
        let old = native_rules::common_tag("different-resource");
        other.content.as_mut().unwrap()["route"]["rules"] = json!([rule(&old)]);
        app.profile_edits.clear();
        app.profile_edits.entry("other".into()).or_default().insert(
            "ruleSetReferences".into(),
            rule_references::metadata(
                other.content.as_ref().unwrap(),
                &[crate::models::rule_references::Replacement {
                    from_tag: old,
                    to_tag: tag,
                }],
            )
            .unwrap(),
        );
        let plan = collect_test(&[p, other], &[s], &[r], &app);
        assert!(plan.shared_resources.iter().any(|v| v
            .field_path
            .as_deref()
            .unwrap_or("")
            .contains("ruleSetReferences")));
    }
    #[test]
    fn global_and_legacy_profile_dns_are_scanned_and_opaque_extensions_fail_closed() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        let tag = native_rules::common_tag(&r.id);
        for variant in 0..4 {
            let mut app = AppConfig::default();
            let mut profiles = vec![p.clone()];
            let value = json!({"config":dns(&tag)});
            match variant {
                0 => app
                    .client_edits
                    .as_mut()
                    .unwrap()
                    .insert("dns".into(), value),
                1 => {
                    app.dns = serde_json::from_value(value).unwrap();
                    None
                }
                2 => {
                    profiles.push(ordinary("other"));
                    app.profile_edits
                        .entry("other".into())
                        .or_default()
                        .insert("dns".into(), value)
                }
                _ => app.client_edits.as_mut().unwrap().insert(
                    "dns".into(),
                    json!({"future":{"type":"rule_set","tag":SECRET}}),
                ),
            };
            let plan = collect_test(
                &profiles,
                std::slice::from_ref(&s),
                std::slice::from_ref(&r),
                &app,
            );
            assert!(
                has(
                    &plan,
                    if variant == 3 {
                        "metadata_invalid"
                    } else {
                        "shared_resource"
                    }
                ),
                "{:?}",
                plan.blockers
            );
            assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
        }
    }
    #[test]
    fn stale_metadata_unknown_native_schema_and_orphan_overrides_are_blocked() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        for variant in 0..3 {
            let mut other = ordinary("other");
            let mut app = AppConfig::default();
            match variant {
                0 => {
                    app.profile_edits.entry("other".into()).or_default().insert(
                        "routeRules".into(),
                        json!({"version":1,"sourceDigest":"0".repeat(64),"rows":[]}),
                    );
                }
                1 => {
                    other.content.as_mut().unwrap()["future_ref"] = json!({"rule_set":SECRET});
                }
                _ => {
                    app.profile_edits
                        .insert("orphan".into(), Default::default());
                }
            }
            let plan = collect_test(
                &[p.clone(), other],
                std::slice::from_ref(&s),
                std::slice::from_ref(&r),
                &app,
            );
            assert!(!plan.blockers.is_empty());
            assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
        }
    }
    #[test]
    fn native_same_tag_with_independent_definition_is_not_resource_ownership() {
        let (p, s) = owned("provider", "one");
        let r = rules(&s);
        let mut other = ordinary("other");
        let v = other.content.as_mut().unwrap();
        v["route"]["rules"] = json!([rule("private-tag")]);
        v["route"]["rule_sets"] = json!([{"tag":"private-tag","type":"file","format":"zrs","path":"/not/opened/independent.zrs"}]);
        let plan = collect_test(&[p, other], &[s], &[r], &AppConfig::default());
        assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    }
    #[test]
    fn component_authority_duplicates_and_resource_budget_fail_closed() {
        assert!(collect(&[], &[], &[], &AppConfig::default(), "plugin-a", &[]).is_err());
        assert!(collect(
            &[],
            &[],
            &[],
            &AppConfig::default(),
            "plugin-a",
            &["same".into(), "same".into()]
        )
        .is_err());
        let (profiles, subs): (Vec<_>, Vec<_>) =
            (0..65).map(|n| owned("provider", &n.to_string())).unzip();
        let plan = collect_test(&profiles, &subs, &[], &AppConfig::default());
        assert!(has(&plan, "resource_budget_exceeded"));
        assert!(plan.profile_ids.is_empty() && plan.subscription_ids.is_empty());
    }
    #[test]
    fn owned_path_only_unknown_or_malformed_sources_block_but_provider_provenance_is_not_io() {
        let (original, subscription) = owned("https://provider.example", "one");
        for variant in 0..5 {
            let mut profile = original.clone();
            match variant {
                0 => {
                    profile.path = Some(format!("/{SECRET}/not-opened"));
                    profile.content = None;
                }
                1 => profile.kernel = "future-kernel".into(),
                2 => {
                    profile.format = "protected".into();
                    profile.content = Some(json!({"secret":SECRET}));
                }
                3 => profile.content = Some(json!({"future_schema":{"rule_set":SECRET}})),
                _ => profile.content = Some(json!([SECRET])),
            }
            let plan = collect_test(
                &[profile],
                std::slice::from_ref(&subscription),
                &[],
                &AppConfig::default(),
            );
            assert!(!plan.blockers.is_empty());
            assert!(plan.profile_ids.is_empty());
            assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
        }
        let mut ordinary_owned = original;
        ordinary_owned.path = Some(format!("https://provider.example/?token={SECRET}"));
        ordinary_owned.content.as_mut().unwrap()["route"]["rule_sets"] = json!([{
            "tag":"own-native", "type":"file", "format":"zrs", "path":"/nonexistent/never-opened.zrs"}]);
        let plan = collect_test(
            &[ordinary_owned],
            &[subscription],
            &[],
            &AppConfig::default(),
        );
        assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
        assert_eq!(plan.profile_ids.len(), 1);
        assert!(!serde_json::to_string(&plan).unwrap().contains(SECRET));
    }
}
