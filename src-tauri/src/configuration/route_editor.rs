//! Fixed local route-rule previews. No IPC, process, TUN or proxy-retarget IO.
use super::{native_validation, route_rules, workspace};
use crate::{
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig, proxy_config::ProxyConfigProfile, route_editor::*,
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

const SCOPE: &str =
    "原生来源与路由规则 schema、引用校验；本地保存待应用，不证明完整运行组合、协议或平台可用";
const MAX_PLANS: usize = 4;
const MAX_ROWS: usize = 1000;
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
        "route_editor_conflict",
        "来源、关系、设置、规则或实例已变化；草稿保留，请明确刷新后重新预览",
    )
}
fn storage() -> AppError {
    failure(
        "route_editor_storage",
        "路由规则本地材料读取失败；草稿保留，请核对存储状态",
    )
}
fn readonly() -> AppError {
    failure(
        "route_editor_read_only",
        "此来源不能在路由规则编辑器修改；请到来源管理或配置工作区查看原因",
    )
}
fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(conflict());
    }
    Ok(())
}
fn admission<'a>(
    profile: &ProxyConfigProfile,
    subscriptions: &[SubscriptionProfile],
) -> Option<&'a str> {
    if profile.managed_source.is_some()
        || subscriptions.iter().any(|s| {
            s.target_proxy_config_id.as_deref() == Some(&profile.id) && s.managed_source.is_some()
        })
    {
        Some("此配置由插件托管；普通路由编辑器不读取或修改正文，请到来源插件管理")
    } else if profile.content.is_none() {
        Some("此来源没有普通可读正文；受保护或路径材料不在此入口读取")
    } else if profile.kernel != "zero" {
        Some("此入口只支持 Zero 原生配置")
    } else {
        None
    }
}
fn source_guard(source: &Value) -> AppResult<()> {
    // The typed native gate rejects dependency paths before RuntimeConfig::parse.
    native_validation::validate_text(&source.to_string()).map(|_| ())
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
fn overlay<'a>(app: &'a AppConfig, id: &str) -> Option<&'a Value> {
    app.profile_edits.get(id).and_then(|e| e.get("routeRules"))
}
fn source_rows(source: &Value) -> AppResult<Vec<Row>> {
    source
        .pointer("/route/rules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, rule)| {
            Ok(Row {
                id: format!("source-{index}-{}", workspace::digest(rule)?),
                enabled: true,
                rule_json: workspace::pretty(rule)?,
            })
        })
        .collect()
}
fn overlay_rows(value: &Value) -> AppResult<Vec<Row>> {
    route_rules::validate(value)?;
    value["rows"]
        .as_array()
        .ok_or_else(conflict)?
        .iter()
        .map(|r| {
            Ok(Row {
                id: r["id"].as_str().ok_or_else(conflict)?.into(),
                enabled: r["enabled"].as_bool().ok_or_else(conflict)?,
                rule_json: workspace::pretty(&r["rule"])?,
            })
        })
        .collect()
}
pub(crate) fn snapshot(
    state: &AppState,
    profile_id: &str,
    from_source: bool,
) -> AppResult<Snapshot> {
    validate_id(profile_id)?;
    {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let subs = lock(state.subscriptions(), "subscription")?;
        let profile = profiles
            .iter()
            .find(|p| p.id == profile_id)
            .ok_or_else(conflict)?;
        let reason = admission(profile, &subs).or_else(|| {
            source_guard(profile.content.as_ref()?).err().map(|_| {
                "来源的 schema、引用或文件依赖不适合此入口；规则集与 GeoIP 依赖不会在此读取"
            })
        });
        if let Some(reason) = reason {
            return Ok(Snapshot {
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                binding: None,
                read_only_reason: Some(reason.into()),
                stale_reason: None,
                has_local_overlay: false,
                rows: vec![],
                targets: vec![],
                source_updated_at_unix_ms: profile.updated_at_unix_ms.to_string(),
                validation_scope: SCOPE.into(),
                pending_apply: true,
            });
        }
    }
    snapshot_from(context(state, profile_id)?, from_source)
}
fn snapshot_from(current: Context, from_source: bool) -> AppResult<Snapshot> {
    let profile_id = current.profile.id.as_str();
    let source = current.profile.content.as_ref().ok_or_else(readonly)?;
    let saved = overlay(&current.app, profile_id);
    let stale = saved.is_some_and(|v| {
        v["sourceDigest"].as_str() != route_rules::source_digest(source).ok().as_deref()
    });
    let rows = if !from_source {
        saved
            .map(overlay_rows)
            .transpose()?
            .unwrap_or(source_rows(source)?)
    } else {
        source_rows(source)?
    };
    if rows.len() > MAX_ROWS || serde_json::to_vec(&rows).map_err(|_| storage())?.len() > MAX_BYTES
    {
        return Err(failure(
            "route_editor_budget",
            "规则数量或正文超过此编辑器预算；请使用配置工作区",
        ));
    }
    let mut targets = vec![
        Target {
            kind: "direct",
            tag: None,
            label: "直连".into(),
        },
        Target {
            kind: "reject",
            tag: None,
            label: "拒绝".into(),
        },
    ];
    // The native parser materializes canonical endpoints into the actual
    // outbound namespace; raw outbounds alone would omit these valid targets.
    // source_guard above has already denied all filesystem dependencies.
    let native = zero_config::RuntimeConfig::parse(&source.to_string())
        .map_err(|_| failure("route_editor_validation", "原生路由目标校验失败；来源保留"))?;
    for tag in native
        .outbounds
        .iter()
        .map(|v| &v.tag)
        .chain(native.outbound_groups.iter().map(|v| &v.tag))
    {
        targets.push(Target {
            kind: "route",
            tag: Some(tag.clone()),
            label: tag.clone(),
        });
    }
    Ok(Snapshot {
        profile_id: current.profile.id.clone(),
        profile_name: current.profile.name.clone(),
        binding: (!stale || from_source)
            .then(|| token(&current))
            .transpose()?,
        read_only_reason: None,
        stale_reason: (stale && !from_source)
            .then(|| "来源已更新；旧覆盖仅供历史查看，请明确使用当前来源重新编辑或恢复来源".into()),
        has_local_overlay: saved.is_some(),
        rows,
        targets,
        source_updated_at_unix_ms: current.profile.updated_at_unix_ms.to_string(),
        validation_scope: SCOPE.into(),
        pending_apply: true,
    })
}
fn ensure_unblocked(owner: &Owner, id: &str) -> AppResult<()> {
    if lock(&owner.plans, "route_editor")?
        .values()
        .any(|p| p.plan.profile_id == id && p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "route_editor_unknown",
            "原规则保存结果未确认；请只读核对原操作，不会重放或建立替代提交",
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
    ensure_unblocked(state.route_editor(), &input.profile_id)?;
    let current = context(state, &input.profile_id)?;
    if token(&current)? != input.binding {
        return Err(conflict());
    }
    if input.rows.len() > MAX_ROWS
        || serde_json::to_vec(&input.rows)
            .map_err(|_| storage())?
            .len()
            > MAX_BYTES
    {
        return Err(failure(
            "route_editor_budget",
            "规则数量或正文超过此编辑器预算",
        ));
    }
    let mut next = current.app.clone();
    let edits = next
        .profile_edits
        .entry(input.profile_id.clone())
        .or_default();
    let canonical;
    if input.mode == Mode::RestoreSource {
        if !input.rows.is_empty() {
            return Err(conflict());
        }
        edits.remove("routeRules");
        canonical = vec![];
    } else {
        let rows = input
            .rows
            .iter()
            .map(|r| {
                let rule: Value = serde_json::from_str(&r.rule_json).map_err(|_| {
                    failure("route_editor_validation", "规则 JSON 不合法；草稿保留")
                })?;
                // Validate original text first: duplicate native fields must not be erased by Value.
                serde_json::from_str::<zero_config::RouteRuleConfig>(&r.rule_json).map_err(
                    |_| failure("route_editor_validation", "规则字段或类型不合法；草稿保留"),
                )?;
                Ok(json!({"id":r.id,"enabled":r.enabled,"rule":rule}))
            })
            .collect::<AppResult<Vec<_>>>()?;
        let source = current.profile.content.as_ref().ok_or_else(readonly)?;
        let metadata =
            json!({"version":1,"sourceDigest":route_rules::source_digest(source)?,"rows":rows});
        route_rules::apply(source, &metadata)?;
        // The preview confirms the exact submitted text; canonical Value is
        // persisted separately and the later snapshot may pretty-print it.
        canonical = input.rows.clone();
        edits.insert("routeRules".into(), metadata);
    }
    if next
        .profile_edits
        .get(&input.profile_id)
        .is_some_and(BTreeMap::is_empty)
    {
        next.profile_edits.remove(&input.profile_id);
    }
    // Validate the same explicit source/global/profile edits consumed on apply,
    // without loading public rule-set artifacts or any source dependency path.
    let (_, edited) = super::local_edits::resolve(
        &next,
        Some(&input.profile_id),
        current.profile.content.as_ref().ok_or_else(readonly)?,
    )
    .map_err(|_| {
        failure(
            "route_editor_validation",
            "客户端配置覆盖校验失败；请检查覆盖和引用，草稿保留",
        )
    })?;
    source_guard(&edited)?;
    if token(&context(state, &input.profile_id)?)? != input.binding {
        return Err(conflict());
    }
    let id = random_id()?;
    let plan = Plan {
        plan_id: id.clone(),
        profile_id: input.profile_id,
        mode: input.mode,
        rows: canonical,
        expires_at_unix_ms: now_unix_ms()
            .saturating_add(TTL.as_millis() as u64)
            .to_string(),
        state: "prepared",
        notice: "仅保存当前配置的客户端规则覆盖；来源不变，运行生效需另行进入工作区预检与应用"
            .into(),
    };
    let mut plans = lock(&state.route_editor().plans, "route_editor")?;
    if plans
        .values()
        .any(|p| p.plan.profile_id == plan.profile_id && p.status.state == "submitted_unknown")
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
                    "route_editor_busy",
                    "已有四个规则预览或未确认保存，请关闭预览后继续",
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
                reason: plan.notice.clone(),
                snapshot: None,
            },
        },
    );
    Ok(plan)
}
pub(crate) fn status(state: &AppState, id: &str) -> AppResult<Status> {
    lock(&state.route_editor().plans, "route_editor")?
        .get(id)
        .map(|p| p.status.clone())
        .ok_or_else(conflict)
}
pub(crate) fn discard(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.route_editor().plans, "route_editor")?;
    if plans
        .get(id)
        .is_some_and(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "route_editor_unknown",
            "原保存结果未确认；不能丢弃记录或重放",
        ));
    }
    plans.remove(id);
    Ok(())
}

/// Caller owns proxy_config_operation. Locks never span an await.
pub(crate) fn save(state: &AppState, id: &str) -> AppResult<Status> {
    let (binding, next, profile_id) = {
        let plans = lock(&state.route_editor().plans, "route_editor")?;
        let p = plans.get(id).ok_or_else(conflict)?;
        if p.status.state != "prepared" {
            return Ok(p.status.clone());
        }
        if p.deadline <= Instant::now() {
            return Err(failure(
                "route_editor_expired",
                "规则预览已过期；请重新预览",
            ));
        }
        if plans.values().any(|other| {
            other.plan.profile_id == p.plan.profile_id && other.status.state == "submitted_unknown"
        }) {
            return Err(conflict());
        }
        (
            p.binding.clone(),
            p.next_app.clone(),
            p.plan.profile_id.clone(),
        )
    };
    let result = publish(state, id, &profile_id, &binding, next);
    let mut plans = lock(&state.route_editor().plans, "route_editor")?;
    let p = plans.get_mut(id).ok_or_else(conflict)?;
    match result {
        Ok(view) => {
            p.status.state = "saved_pending_apply";
            p.status.reason = "客户端规则覆盖已保存，运行尚未应用；请进入工作区预检和应用".into();
            p.status.snapshot = Some(view);
        }
        Err(error) => {
            if p.status.state != "submitted_unknown" {
                p.status.state = "rejected";
                p.status.reason = error.message;
            } else {
                p.status.reason =
                    "本地规则保存已进入提交，但结果未确认；保留原计划，只读核对，不会自动重放"
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
                        let mut plans = lock(&state.route_editor().plans, "route_editor")?;
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
                        .route_editor()
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
    if active.as_ref().is_some_and(|p| p.id == profile_id) {
        state.client_core_configuration_committed(active.as_ref());
    }
    #[cfg(test)]
    {
        let hook = lock(&state.route_editor().after_publish, "route_test_hook")?.take();
        if let Some(run) = hook {
            run(state);
        }
    }
    // A subsequent writer's snapshot is not the original operation's receipt.
    // Check exact publication facts before delivering an immutable local view.
    let after = context(state, profile_id)?;
    let expected_generation = binding.generation.saturating_add(u64::from(
        active.as_ref().is_some_and(|p| p.id == profile_id),
    ));
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
    snapshot_from(after, false)
}

#[cfg(test)]
mod tests;
