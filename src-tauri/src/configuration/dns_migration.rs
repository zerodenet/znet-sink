//! Fixed, host-owned DNS deletion/rename reference migration. Local publication only.
use super::workspace;
use crate::{
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig, dns_migration::*, proxy_config::ProxyConfigProfile,
        subscription::SubscriptionProfile,
    },
    services::{
        app_config_store,
        app_database::workspace_recovery,
        common::{lock, now_unix_ms},
        domain_store,
        file_history::{self, Domain},
    },
    state::app_state::AppState,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const NOTICE: &str = "迁移保存为全局客户端 DNS 覆盖，影响所有配置；来源不变，运行生效需另行工作区预检与应用。仅验证已知 DNS 结构与引用，未编译规则/GeoIP 依赖、未验证其它托管或受保护来源及实际平台运行。";
const MAX_PLANS: usize = 4;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(300);
#[derive(Default)]
pub(crate) struct Owner {
    plans: Mutex<BTreeMap<String, Prepared>>,
    #[cfg(test)]
    fail_after_write: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    after_publish: Mutex<Option<fn(&AppState)>>,
}
struct Prepared {
    binding: workspace::Binding,
    next_app: AppConfig,
    plan: Plan,
    status: Status,
    deadline: Instant,
}
struct Context {
    profile: ProxyConfigProfile,
    app: AppConfig,
    binding: workspace::Binding,
}
fn failure(code: &'static str, message: &str) -> AppError {
    AppError {
        code,
        message: message.into(),
        details: None,
    }
}
fn conflict() -> AppError {
    failure(
        "dns_migration_conflict",
        "来源、关系、设置、DNS或实例已变化；草稿保留，请明确刷新后重新预览",
    )
}
fn storage() -> AppError {
    failure(
        "dns_migration_storage",
        "DNS本地材料读取失败；草稿保留，请核对存储状态",
    )
}
fn readonly() -> AppError {
    failure(
        "dns_migration_read_only",
        "此来源不能在DNS编辑器修改；请到来源管理或配置工作区查看原因",
    )
}
fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(conflict());
    }
    Ok(())
}
fn admission(
    profile: &ProxyConfigProfile,
    subscriptions: &[SubscriptionProfile],
) -> Option<&'static str> {
    if profile.managed_source.is_some()
        || subscriptions.iter().any(|s| {
            s.target_proxy_config_id.as_deref() == Some(&profile.id) && s.managed_source.is_some()
        })
    {
        Some("此配置由插件托管；普通 DNS 迁移入口不读取或修改正文，请到来源插件管理")
    } else if profile.content.is_none() {
        Some("此来源没有普通可读正文；受保护或路径材料不在此入口读取")
    } else if profile.kernel != "zero" {
        Some("此入口只支持 Zero 原生配置")
    } else {
        None
    }
}
fn source_guard(source: &Value) -> AppResult<()> {
    // Deserialize only. RuntimeConfig::parse/validate would read rule/GeoIP files.
    serde_json::from_value::<zero_config::RuntimeConfig>(source.clone())
        .map(|_| ())
        .map_err(|_| {
            failure(
                "dns_migration_validation",
                "来源 schema 不适合此 DNS 迁移入口；材料保留",
            )
        })
}
fn token(context: &Context) -> AppResult<String> {
    workspace::digest(&(&context.profile.id, &context.binding))
}
fn context(state: &AppState, id: &str) -> AppResult<Context> {
    validate_id(id)?;
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let profile = profiles.iter().find(|p| p.id == id).ok_or_else(conflict)?;
    // No source clone, projection or temporary DTO before source admission.
    if admission(profile, &subs).is_some() {
        return Err(readonly());
    }
    workspace::ensure_unblocked(state, id)?;
    source_guard(profile.content.as_ref().ok_or_else(readonly)?)?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    let snapshot = state.client_core_snapshot();
    if snapshot.scope_generation.0 == 0 || snapshot.scope_generation.0 > 9_007_199_254_740_991 {
        return Err(conflict());
    }
    let dir = workspace::recovery::directory(state)?;
    let app_file = file_history::acquire(&dir.join("app-config.json"), Domain::AppConfig)
        .map_err(|_| storage())?;
    let rules_file = file_history::acquire(&dir.join("rule-sets.json"), Domain::RuleSets)
        .map_err(|_| storage())?;
    if app_config_store::read_current_locked(&app_file).map_err(|_| storage())? != *app
        || workspace::digest(
            &domain_store::read_rule_sets_locked(&rules_file).map_err(|_| storage())?,
        )? != workspace::digest(&*rules)?
    {
        return Err(conflict());
    }
    let binding = workspace::Binding {
        host_session_id: state.host_session_id()?.into(),
        domain_epoch: workspace_recovery::current_epoch(&dir).map_err(|_| storage())?,
        app_history: app_file.token().map_err(|_| storage())?,
        rules_history: rules_file.token().map_err(|_| storage())?,
        scope: snapshot.scope,
        generation: snapshot.scope_generation.0,
        source_digest: workspace::digest(profile)?,
        graph_digest: workspace::digest(&(&*profiles, &*subs))?,
        app_digest: workspace::digest(&*app)?,
        rules_digest: workspace::digest(&*rules)?,
    };
    Ok(Context {
        profile: profile.clone(),
        app: app.clone(),
        binding,
    })
}
fn current_dns(context: &Context) -> AppResult<Value> {
    saved_dns(
        &context.app,
        Some(&context.profile.id),
        context.profile.content.as_ref(),
    )
}
fn saved_dns(saved: &AppConfig, id: Option<&str>, source: Option<&Value>) -> AppResult<Value> {
    let mut app = saved.clone();
    super::local_edits::migrate_legacy(&mut app, id);
    if let Some(dns) = app.client_edits.as_ref().and_then(|e| e.get("dns")) {
        let object = dns.as_object().ok_or_else(|| {
            failure(
                "dns_migration_validation",
                "保存的 DNS 覆盖结构不合法；原材料保留",
            )
        })?;
        if object
            .keys()
            .any(|k| !matches!(k.as_str(), "enabled" | "config" | "dnsHijack"))
        {
            return Err(failure(
                "dns_migration_validation",
                "保存的 DNS 覆盖含未识别字段；原材料保留，请先明确处理",
            ));
        }
        serde_json::from_value::<crate::models::app_config::AppDnsConfig>(dns.clone()).map_err(
            |_| {
                failure(
                    "dns_migration_validation",
                    "保存的 DNS 覆盖类型不合法；原材料保留",
                )
            },
        )?;
        return Ok(dns.clone());
    }
    if let Some(dns) = source.and_then(|source| source.pointer("/runtime/dns")) {
        let hijack = app
            .client_edits
            .as_ref()
            .and_then(|e| e.get("tun.dnsHijack"))
            .and_then(Value::as_bool)
            .or_else(|| {
                source
                    .and_then(|source| source.pointer("/runtime/tun/dns_hijack"))
                    .and_then(Value::as_bool)
            })
            .unwrap_or(app.tun.dns_hijack);
        return Ok(json!({"enabled":true,"config":dns,"dnsHijack":hijack}));
    }
    serde_json::to_value(&app.dns).map_err(|_| storage())
}

/// Narrow legacy shortcut gate. Call before status queries, runtime effects,
/// validation that could compile source dependencies, or settings publication.
/// Full workspace/settings-transfer fixed previews have their own publishers.
pub(crate) fn require_legacy_server_preserved(
    state: &AppState,
    old: &AppConfig,
    next: &AppConfig,
) -> AppResult<()> {
    let edit = |app: &AppConfig| {
        app.client_edits
            .as_ref()
            .and_then(|e| e.get("dns"))
            .cloned()
    };
    if edit(old) == edit(next)
        && old.dns == next.dns
        && old.overrides.dns == next.overrides.dns
        && old.client_edits.is_some() == next.client_edits.is_some()
    {
        return Ok(());
    }
    ensure_unblocked(state.dns_migration(), "")?;
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let active = profiles.iter().find(|p| p.active);
    let id = active.map(|p| p.id.as_str());
    let mut old_projected = old.clone();
    let mut next_projected = next.clone();
    super::local_edits::migrate_legacy(&mut old_projected, id);
    super::local_edits::migrate_legacy(&mut next_projected, id);
    let needs_source = edit(&old_projected).is_none() || edit(&next_projected).is_none();
    if needs_source && active.is_some_and(|p| admission(p, &subs).is_some()) {
        return Err(readonly());
    }
    let source = needs_source
        .then(|| active.and_then(|p| p.content.as_ref()))
        .flatten();
    let names = |wrapper: Value| -> AppResult<std::collections::BTreeSet<String>> {
        match wrapper.get("config") {
            None | Some(Value::Null) => Ok(Default::default()),
            Some(config) => Ok(config
                .get("servers")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    failure(
                        "dns_migration_validation",
                        "保存的 DNS 服务器结构不合法；原材料保留",
                    )
                })?
                .keys()
                .cloned()
                .collect()),
        }
    };
    let before = names(saved_dns(&old_projected, id, source)?)?;
    let after = names(saved_dns(&next_projected, id, source)?)?;
    if !before.is_subset(&after) {
        return Err(failure(
            "dns_migration_preview_required",
            "删除或重命名 DNS 服务器需要专用引用迁移预览；此快捷入口未保存或提交运行变更",
        ));
    }
    Ok(())
}
fn dns_config(wrapper: &Value) -> AppResult<&Value> {
    wrapper
        .get("config")
        .filter(|v| v.is_object())
        .ok_or_else(|| {
            failure(
                "dns_migration_unavailable",
                "当前保存材料没有可迁移的 DNS 配置",
            )
        })
}
pub(crate) fn snapshot(state: &AppState, profile_id: &str) -> AppResult<Snapshot> {
    validate_id(profile_id)?;
    {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let subs = lock(state.subscriptions(), "subscription")?;
        let profile = profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or_else(conflict)?;
        let reason = admission(profile, &subs).or_else(|| {
            source_guard(profile.content.as_ref()?)
                .err()
                .map(|_| "来源 schema 不适合本入口；不会读取规则或 GeoIP 依赖")
        });
        if let Some(reason) = reason {
            return Ok(Snapshot {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                binding: None,
                read_only_reason: Some(reason.into()),
                servers: vec![],
                notice: NOTICE.into(),
                pending_apply: true,
            });
        }
    }
    snapshot_from(context(state, profile_id)?)
}
fn snapshot_from(current: Context) -> AppResult<Snapshot> {
    let wrapper = current_dns(&current)?;
    let config = match dns_config(&wrapper) {
        Ok(value) => value,
        Err(_) => {
            return Ok(Snapshot {
                profile_id: current.profile.id.clone(),
                profile_name: current.profile.name.clone(),
                binding: None,
                read_only_reason: Some("当前保存材料没有可迁移的 DNS 配置".into()),
                servers: vec![],
                notice: NOTICE.into(),
                pending_apply: true,
            })
        }
    };
    let servers = transforms::servers(config)?;
    Ok(Snapshot {
        profile_id: current.profile.id.clone(),
        profile_name: current.profile.name.clone(),
        binding: Some(token(&current)?),
        read_only_reason: None,
        servers,
        notice: NOTICE.into(),
        pending_apply: true,
    })
}
fn ensure_unblocked(owner: &Owner, _id: &str) -> AppResult<()> {
    if lock(&owner.plans, "dns_migration")?
        .values()
        .any(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "dns_migration_unknown",
            "原DNS保存结果未确认；请只读核对原操作，不会重放或建立替代提交",
        ));
    }
    Ok(())
}
fn random_id() -> AppResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| storage())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(crate) fn preview(state: &AppState, input: PreviewInput) -> AppResult<Plan> {
    ensure_unblocked(state.dns_migration(), &input.profile_id)?;
    let current = context(state, &input.profile_id)?;
    if token(&current)? != input.binding {
        return Err(conflict());
    }
    let mut wrapper = current_dns(&current)?;
    let (candidate, references) = if let Some(name) = &input.rename_to {
        if input.replacement_name.is_some() {
            return Err(failure(
                "dns_migration_name",
                "重命名不能同时指定已有替代服务器；请明确选择一个操作",
            ));
        }
        require_rename_scope(state, &current)?;
        transforms::rename(dns_config(&wrapper)?, &input.server_name, name)?
    } else {
        transforms::migrate(
            dns_config(&wrapper)?,
            &input.server_name,
            input.replacement_name.as_deref(),
        )?
    };
    wrapper["config"] = candidate;
    let mut next = current.app.clone();
    super::local_edits::migrate_legacy(&mut next, Some(&input.profile_id));
    next.client_edits
        .get_or_insert_with(BTreeMap::new)
        .insert("dns".into(), wrapper);
    if token(&context(state, &input.profile_id)?)? != input.binding {
        return Err(conflict());
    }
    let id = random_id()?;
    let plan = Plan {
        plan_id: id.clone(),
        profile_id: input.profile_id,
        server_name: input.server_name,
        replacement_name: input.replacement_name,
        rename_to: input.rename_to,
        references,
        notice: NOTICE.into(),
        state: "prepared",
        expires_at_unix_ms: now_unix_ms()
            .saturating_add(TTL.as_millis() as u64)
            .to_string(),
    };
    let mut plans = lock(&state.dns_migration().plans, "dns_migration")?;
    if plans
        .values()
        .any(|p| p.status.state == "submitted_unknown")
    {
        return Err(conflict());
    }
    plans.retain(|_, p| p.status.state == "submitted_unknown" || p.deadline > Instant::now());
    while plans.len() >= MAX_PLANS {
        let evict = plans
            .iter()
            .find(|(_, p)| matches!(p.status.state, "saved_pending_apply" | "rejected"))
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                failure(
                    "dns_migration_busy",
                    "已有四个 DNS 预览或未确认保存，请关闭预览后继续",
                )
            })?;
        plans.remove(&evict);
    }
    plans.insert(
        id.clone(),
        Prepared {
            binding: current.binding,
            next_app: next,
            plan: plan.clone(),
            deadline: Instant::now() + TTL,
            status: Status {
                plan_id: id,
                profile_id: plan.profile_id.clone(),
                state: "prepared",
                reason: NOTICE.into(),
                snapshot: None,
            },
        },
    );
    Ok(plan)
}

/// An existing global override is one closed DNS object. Promoting a source
/// DNS object is different: do not enlarge this rename into a multi-profile
/// source migration or silently bypass an existing source projection.
fn require_rename_scope(state: &AppState, current: &Context) -> AppResult<()> {
    // migrate_legacy prefers profile DNS before filling legacy global values
    // with entry.or_insert. A global flag alone is not proof of provenance.
    let preferred_legacy_profile_dns = current.app.client_edits.is_none()
        && current
            .app
            .profile_edits
            .get(&current.profile.id)
            .is_some_and(|edits| edits.contains_key("dns"));
    let explicit_global = current
        .app
        .client_edits
        .as_ref()
        .is_some_and(|edits| edits.contains_key("dns"))
        || (current.app.overrides.dns && !preferred_legacy_profile_dns);
    if explicit_global {
        return Ok(());
    }
    if lock(state.proxy_configs(), "proxy_config")?.len() != 1
        || current
            .app
            .profile_edits
            .get(&current.profile.id)
            .is_some_and(|edits| edits.contains_key("ruleSetReferences"))
    {
        return Err(failure(
            "dns_migration_scope",
            "来源 DNS 晋升全局会涉及其它配置或已有引用覆盖；本入口不迁移跨配置/托管来源，请先在配置工作区明确处理",
        ));
    }
    Ok(())
}
pub(crate) fn status(state: &AppState, id: &str) -> AppResult<Status> {
    lock(&state.dns_migration().plans, "dns_migration")?
        .get(id)
        .map(|p| p.status.clone())
        .ok_or_else(conflict)
}
pub(crate) fn discard(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.dns_migration().plans, "dns_migration")?;
    if plans
        .get(id)
        .is_some_and(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "dns_migration_unknown",
            "原保存结果未确认；不能丢弃记录或重放",
        ));
    }
    plans.remove(id);
    Ok(())
}

/// Caller owns proxy_config_operation. Locks never span an await.
pub(crate) fn save(state: &AppState, id: &str) -> AppResult<Status> {
    let (binding, next, profile_id) = {
        let plans = lock(&state.dns_migration().plans, "dns_migration")?;
        let p = plans.get(id).ok_or_else(conflict)?;
        if p.status.state != "prepared" {
            return Ok(p.status.clone());
        }
        if p.deadline <= Instant::now() {
            return Err(failure(
                "dns_migration_expired",
                "DNS预览已过期；请重新预览",
            ));
        }
        if plans
            .values()
            .any(|other| other.status.state == "submitted_unknown")
        {
            return Err(conflict());
        }
        (
            p.binding.clone(),
            p.next_app.clone(),
            p.plan.profile_id.clone(),
        )
    };
    let result = publish(state, id, &profile_id, &binding, next);
    let mut plans = lock(&state.dns_migration().plans, "dns_migration")?;
    let p = plans.get_mut(id).ok_or_else(conflict)?;
    match result {
        Ok(view) => {
            p.status.state = "saved_pending_apply";
            p.status.reason = "客户端DNS覆盖已保存，运行尚未应用；请进入工作区预检和应用".into();
            p.status.snapshot = Some(view);
        }
        Err(error) => {
            if p.status.state != "submitted_unknown" {
                p.status.state = "rejected";
                p.status.reason = error.message;
            } else {
                p.status.reason =
                    "本地DNS保存已进入提交，但结果未确认；保留原计划，只读核对，不会自动重放"
                        .into();
            }
        }
    }
    Ok(p.status.clone())
}
fn publish(
    state: &AppState,
    id: &str,
    profile_id: &str,
    binding: &workspace::Binding,
    next: AppConfig,
) -> AppResult<Snapshot> {
    let next_digest = workspace::digest(&next)?;
    let current = context(state, profile_id)?;
    if workspace::digest(&current.binding)? != workspace::digest(binding)? {
        return Err(conflict());
    }
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let mut app = lock(state.app_config(), "app_config")?;
    if workspace::digest(&(&*profiles, &*subs))? != binding.graph_digest
        || workspace::digest(&*rules)? != binding.rules_digest
        || workspace::digest(&*app)? != binding.app_digest
    {
        return Err(conflict());
    }
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(conflict)?;
    if admission(profile, &subs).is_some() {
        return Err(readonly());
    }
    let active = profiles.iter().find(|p| p.active).cloned();
    let dir = workspace::recovery::directory(state)?;
    let (mut app_file, _rules_file) = workspace::recovery::guard_files(
        &dir,
        &binding.app_history,
        &binding.rules_history,
        &binding.app_digest,
        &binding.rules_digest,
    )
    .map_err(|_| conflict())?;
    let written_history =
        state.with_client_scope_generation(&binding.scope, binding.generation, || {
            if state.host_session_id()? != binding.host_session_id {
                return Err(conflict());
            }
            workspace_recovery::with_graph_epoch(
                &dir,
                binding.domain_epoch,
                &binding.graph_digest,
                || {
                    {
                        let mut plans = lock(&state.dns_migration().plans, "dns_migration")?;
                        let plan = plans.get_mut(id).ok_or_else(conflict)?;
                        if plan.status.state != "prepared" || plan.deadline <= Instant::now() {
                            return Err(conflict());
                        }
                        plan.status.state = "submitted_unknown";
                    }
                    let receipt = app_config_store::save_owned_locked(&mut app_file, &next, id)
                        .map_err(|_| storage())?;
                    #[cfg(test)]
                    if state
                        .dns_migration()
                        .fail_after_write
                        .swap(false, std::sync::atomic::Ordering::SeqCst)
                    {
                        return Err(storage());
                    }
                    *app = next;
                    Ok(receipt.after)
                },
            )
        })?;
    drop(_rules_file);
    drop(app_file);
    drop(app);
    drop(rules);
    drop(subs);
    drop(profiles);
    state.client_core_configuration_committed(active.as_ref());
    #[cfg(test)]
    {
        let hook = lock(&state.dns_migration().after_publish, "dns_test_hook")?.take();
        if let Some(run) = hook {
            run(state);
        }
    }
    // A subsequent writer's snapshot is not the original operation's receipt.
    // Check exact publication facts before delivering an immutable local view.
    let after = context(state, profile_id)?;
    let expected_generation = binding.generation.saturating_add(1);
    if after.binding.app_digest != next_digest
        || after.binding.app_history != written_history
        || after.binding.rules_history != binding.rules_history
        || after.binding.source_digest != binding.source_digest
        || after.binding.graph_digest != binding.graph_digest
        || after.binding.domain_epoch != binding.domain_epoch
        || after.binding.rules_digest != binding.rules_digest
        || after.binding.scope != binding.scope
        || after.binding.generation != expected_generation
        || after.binding.host_session_id != binding.host_session_id
    {
        return Err(conflict());
    }
    snapshot_from(after)
}

#[cfg(test)]
mod tests;
mod transforms;
