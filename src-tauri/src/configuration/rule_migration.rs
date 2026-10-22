//! Fixed, local rule-resource retirement. References are published before the
//! old asset is removed; these two files are deliberately not called atomic.
use super::{rule_references, rules as native_rules, workspace};
use crate::{
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig, proxy_config::ProxyConfigProfile, rule_migration::*,
        rule_set::RuleSetProfile, subscription::SubscriptionProfile,
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
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(300);
const NOTICE: &str = "仅迁移已列出的普通配置客户端引用并删除旧规则集记录；先保存引用、后删除记录，非跨文件原子事务。来源和产物文件保留，运行尚未应用，请另行进入配置工作区预检与应用";
#[derive(Default)]
pub(crate) struct Owner {
    plans: Mutex<BTreeMap<String, Prepared>>,
    #[cfg(test)]
    fail_after_app: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    fail_after_rules: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    after_publish: Mutex<Option<fn(&AppState)>>,
}
struct Prepared {
    binding: workspace::Binding,
    candidate: LocalCandidate,
    plan: Plan,
    status: Status,
    deadline: Instant,
}
#[derive(Clone)]
struct LocalCandidate {
    app: AppConfig,
    rules: Vec<RuleSetProfile>,
    target_proof: Option<(String, String)>,
}
struct Context {
    profiles: Vec<ProxyConfigProfile>,
    subscriptions: Vec<SubscriptionProfile>,
    rules: Vec<RuleSetProfile>,
    app: AppConfig,
    profile_name: String,
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
        "rule_migration_conflict",
        "来源、引用、资源、关系或本地文件代次已变化；原选择保留，请明确刷新后重新预览",
    )
}
fn storage() -> AppError {
    failure(
        "rule_migration_storage",
        "本地迁移材料无法核对；保留原操作，不会重放或回滚",
    )
}
fn valid_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        Err(conflict())
    } else {
        Ok(())
    }
}
fn token(current: &Context, profile: &str, resource: &str) -> AppResult<String> {
    workspace::digest(&(profile, resource, &current.binding))
}
fn context(state: &AppState, profile_id: &str) -> AppResult<Context> {
    valid_id(profile_id)?;
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(conflict)?;
    // Ownership-specific blockers are returned by the inventory; context never
    // serializes configuration material into a public DTO.
    workspace::ensure_unblocked(state, profile_id)?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    let scope = state.client_core_snapshot();
    if scope.scope_generation.0 == 0 || scope.scope_generation.0 > 9_007_199_254_740_991 {
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
        scope: scope.scope,
        generation: scope.scope_generation.0,
        source_digest: workspace::digest(profile)?,
        graph_digest: workspace::digest(&(&*profiles, &*subscriptions))?,
        app_digest: workspace::digest(&*app)?,
        rules_digest: workspace::digest(&*rules)?,
    };
    // A preview must describe the persisted graph too; a stale in-memory
    // source is not made authoritative merely by reading its current epoch.
    workspace_recovery::with_graph_epoch(
        &dir,
        binding.domain_epoch,
        &binding.graph_digest,
        || Ok(()),
    )?;
    Ok(Context {
        profiles: profiles.clone(),
        subscriptions: subscriptions.clone(),
        rules: rules.clone(),
        app: app.clone(),
        profile_name: profile.name.clone(),
        binding,
    })
}
pub(crate) fn snapshot(
    state: &AppState,
    profile_id: &str,
    rule_set_id: &str,
) -> AppResult<Snapshot> {
    valid_id(rule_set_id)?;
    let current = context(state, profile_id)?;
    let binding = Some(token(&current, profile_id, rule_set_id)?);
    let resource = current.rules.iter().find(|r| r.id == rule_set_id);
    let mut result = Snapshot {
        profile_id: profile_id.into(),
        profile_name: current.profile_name.clone(),
        rule_set_id: rule_set_id.into(),
        rule_set_name: resource.map(|r| r.name.clone()),
        resource_exists: resource.is_some(),
        tag: native_rules::common_tag(rule_set_id),
        binding,
        references: vec![],
        blockers: vec![],
        targets: vec![],
        common_binding_removed: false,
        notice: NOTICE.into(),
        pending_apply: true,
    };
    if resource.is_none() {
        return Ok(result);
    }
    let inspect = rule_references::prepare(
        &current.app,
        &current.profiles,
        &current.subscriptions,
        &current.rules,
        Some(profile_id),
        rule_set_id,
        None,
    )?;
    result.references = inspect.references;
    result.blockers = inspect.blockers;
    result.common_binding_removed = inspect.common_binding_removed;
    let eligible = native_rules::eligible_profiles(&current.rules)
        .filter(|r| r.id != rule_set_id && !r.built_in)
        .collect::<Vec<_>>();
    if eligible.len() > 128 {
        return Err(failure(
            "rule_migration_budget",
            "可选替代资源超过128项；未截断目标集合，请先减少资源范围",
        ));
    }
    for target in eligible {
        result.targets.push(Target {
            id: target.id.clone(),
            name: target.name.clone(),
            tag: native_rules::common_tag(&target.id),
            // Declaration eligibility is cheap. Full source/target admission
            // runs once after explicit selection, rather than rescanning up
            // to 16MiB of material for every displayed target.
            unavailable_reason: None,
        });
    }
    Ok(result)
}
fn ensure_unblocked(owner: &Owner) -> AppResult<()> {
    if lock(&owner.plans, "rule_migration")?
        .values()
        .any(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "rule_migration_unknown",
            "存在原规则集迁移结果未确认；只核对原操作，不创建或重放新迁移",
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
    ensure_unblocked(state.rule_migration())?;
    valid_id(&input.rule_set_id)?;
    if let Some(id) = &input.target_id {
        valid_id(id)?;
    }
    let current = context(state, &input.profile_id)?;
    if token(&current, &input.profile_id, &input.rule_set_id)? != input.binding {
        return Err(conflict());
    }
    let candidate = rule_references::prepare(
        &current.app,
        &current.profiles,
        &current.subscriptions,
        &current.rules,
        Some(&input.profile_id),
        &input.rule_set_id,
        input.target_id.as_deref(),
    )?;
    if !candidate.references.is_empty() && input.target_id.is_none() {
        return Err(failure(
            "rule_migration_target_required",
            "这些引用仍在使用旧规则集，请明确选择本次替代资源",
        ));
    }
    if !candidate.blockers.is_empty() {
        return Err(failure(
            "rule_migration_blocked",
            "仍有不可迁移的来源、引用或目标；请查看引用清单和受限范围，原材料未修改",
        ));
    }
    let resource = current
        .rules
        .iter()
        .find(|r| r.id == input.rule_set_id)
        .ok_or_else(conflict)?;
    let target_name = input
        .target_id
        .as_ref()
        .and_then(|id| current.rules.iter().find(|r| &r.id == id))
        .map(|r| r.name.clone());
    let target_proof = input
        .target_id
        .as_ref()
        .map(|id| {
            let target = current
                .rules
                .iter()
                .find(|r| &r.id == id)
                .ok_or_else(conflict)?;
            let proof = crate::services::rule_migration_artifact::verify(
                &workspace::recovery::directory(state)?,
                target,
            )?;
            Ok::<_, AppError>((id.clone(), proof))
        })
        .transpose()?;
    let next_rules = current
        .rules
        .iter()
        .filter(|r| r.id != input.rule_set_id)
        .cloned()
        .collect::<Vec<_>>();
    if token(
        &context(state, &input.profile_id)?,
        &input.profile_id,
        &input.rule_set_id,
    )? != input.binding
    {
        return Err(conflict());
    }
    let id = random_id()?;
    let plan = Plan {
        plan_id: id.clone(),
        profile_id: input.profile_id,
        rule_set_id: input.rule_set_id,
        rule_set_name: resource.name.clone(),
        target_id: input.target_id,
        target_name,
        from_tag: candidate.from_tag,
        to_tag: candidate.to_tag,
        references: candidate.references,
        common_binding_removed: candidate.common_binding_removed,
        expires_at_unix_ms: now_unix_ms()
            .saturating_add(TTL.as_millis() as u64)
            .to_string(),
        state: "prepared",
        notice: NOTICE.into(),
    };
    let mut plans = lock(&state.rule_migration().plans, "rule_migration")?;
    if plans
        .values()
        .any(|p| p.status.state == "submitted_unknown")
    {
        return Err(conflict());
    }
    plans.retain(|_, p| p.status.state == "submitted_unknown" || p.deadline > Instant::now());
    while plans.len() >= 4 {
        let evict = plans
            .iter()
            .find(|(_, p)| matches!(p.status.state, "saved_pending_apply" | "rejected"))
            .map(|(id, _)| id.clone())
            .ok_or_else(|| failure("rule_migration_busy", "已有四份迁移预览，请先关闭原预览"))?;
        plans.remove(&evict);
    }
    plans.insert(
        id.clone(),
        Prepared {
            binding: current.binding,
            candidate: LocalCandidate {
                app: candidate.app,
                rules: next_rules,
                target_proof,
            },
            plan: plan.clone(),
            deadline: Instant::now() + TTL,
            status: Status {
                plan_id: id,
                profile_id: plan.profile_id.clone(),
                rule_set_id: plan.rule_set_id.clone(),
                state: "prepared",
                reason: NOTICE.into(),
                app_config_published: false,
                rule_set_removed: false,
                after_binding: None,
            },
        },
    );
    Ok(plan)
}
pub(crate) fn status(state: &AppState, id: &str) -> AppResult<Status> {
    lock(&state.rule_migration().plans, "rule_migration")?
        .get(id)
        .map(|p| p.status.clone())
        .ok_or_else(conflict)
}
pub(crate) fn discard(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.rule_migration().plans, "rule_migration")?;
    if plans
        .get(id)
        .is_some_and(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "rule_migration_unknown",
            "结果未确认的原迁移不能丢弃或重放",
        ));
    }
    plans.remove(id);
    Ok(())
}
/// Caller holds proxy_config_operation; all synchronous locks remain local.
pub(crate) fn save(state: &AppState, id: &str) -> AppResult<Status> {
    let (binding, candidate, profile, resource) = {
        let plans = lock(&state.rule_migration().plans, "rule_migration")?;
        let p = plans.get(id).ok_or_else(conflict)?;
        if p.status.state != "prepared" {
            return Ok(p.status.clone());
        }
        if p.deadline <= Instant::now() {
            return Err(failure(
                "rule_migration_expired",
                "迁移预览已过期，请明确重新预览",
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
            p.candidate.clone(),
            p.plan.profile_id.clone(),
            p.plan.rule_set_id.clone(),
        )
    };
    let result = publish(state, id, &profile, &resource, &binding, candidate);
    let mut plans = lock(&state.rule_migration().plans, "rule_migration")?;
    let p = plans.get_mut(id).ok_or_else(conflict)?;
    match result {
        Ok(binding) => {
            p.status.state = "saved_pending_apply";
            p.status.reason="引用迁移与旧规则集记录删除已本地保存，运行尚未应用；产物保留，请进入工作区预检与应用".into();
            p.status.after_binding = Some(binding);
        }
        Err(error) => {
            if p.status.state != "submitted_unknown" {
                p.status.state = "rejected";
                p.status.reason = error.message;
            } else {
                p.status.reason="本地迁移已进入提交，但原结果未确认；已保存引用和旧记录删除可能处于不同阶段。保留原操作，只读核对，不会回滚或重放".into();
            }
        }
    }
    Ok(p.status.clone())
}
fn publish(
    state: &AppState,
    id: &str,
    profile_id: &str,
    resource_id: &str,
    binding: &workspace::Binding,
    candidate: LocalCandidate,
) -> AppResult<String> {
    let LocalCandidate {
        app: next_app,
        rules: next_rules,
        target_proof,
    } = candidate;
    let current = context(state, profile_id)?;
    if workspace::digest(&current.binding)? != workspace::digest(binding)? {
        return Err(conflict());
    }
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut rules = lock(state.rule_sets(), "rule_set")?;
    let mut app = lock(state.app_config(), "app_config")?;
    if workspace::digest(&(&*profiles, &*subscriptions))? != binding.graph_digest
        || workspace::digest(&*rules)? != binding.rules_digest
        || workspace::digest(&*app)? != binding.app_digest
    {
        return Err(conflict());
    }
    let next_app_digest = workspace::digest(&next_app)?;
    let next_rules_digest = workspace::digest(&next_rules)?;
    let app_changed = next_app_digest != binding.app_digest;
    let active = profiles.iter().find(|p| p.active).cloned();
    let dir = workspace::recovery::directory(state)?;
    verify_target(&dir, &rules, target_proof.as_ref())?;
    let (mut app_file, mut rules_file) = workspace::recovery::guard_files(
        &dir,
        &binding.app_history,
        &binding.rules_history,
        &binding.app_digest,
        &binding.rules_digest,
    )
    .map_err(|_| conflict())?;
    let result = state.with_client_scope_generation(&binding.scope, binding.generation, || {
        if state.host_session_id()? != binding.host_session_id {
            return Err(conflict());
        }
        workspace_recovery::with_graph_epoch(
            &dir,
            binding.domain_epoch,
            &binding.graph_digest,
            || {
                {
                    let mut plans = lock(&state.rule_migration().plans, "rule_migration")?;
                    let p = plans.get_mut(id).ok_or_else(conflict)?;
                    if p.status.state != "prepared" || p.deadline <= Instant::now() {
                        return Err(conflict());
                    }
                    p.status.state = "submitted_unknown";
                }
                // Every published prefix is safe: the replacement already exists,
                // and the old resource remains until all references are saved.
                let app_history = if app_changed {
                    let receipt = app_config_store::save_owned_locked(&mut app_file, &next_app, id)
                        .map_err(|_| storage())?;
                    *app = next_app;
                    lock(&state.rule_migration().plans, "rule_migration")?
                        .get_mut(id)
                        .ok_or_else(conflict)?
                        .status
                        .app_config_published = true;
                    #[cfg(test)]
                    if state
                        .rule_migration()
                        .fail_after_app
                        .swap(false, std::sync::atomic::Ordering::SeqCst)
                    {
                        return Err(storage());
                    }
                    receipt.after
                } else {
                    binding.app_history.clone()
                };
                let receipt =
                    domain_store::save_rule_sets_owned_locked(&mut rules_file, &next_rules, id)
                        .map_err(|_| storage())?;
                *rules = next_rules;
                lock(&state.rule_migration().plans, "rule_migration")?
                    .get_mut(id)
                    .ok_or_else(conflict)?
                    .status
                    .rule_set_removed = true;
                #[cfg(test)]
                if state
                    .rule_migration()
                    .fail_after_rules
                    .swap(false, std::sync::atomic::Ordering::SeqCst)
                {
                    return Err(storage());
                }
                Ok((app_history, receipt.after))
            },
        )
    });
    let local_published =
        status(state, id).is_ok_and(|s| s.app_config_published || s.rule_set_removed);
    drop(rules_file);
    drop(app_file);
    drop(app);
    drop(rules);
    drop(subscriptions);
    drop(profiles);
    // Even a known partial local publication invalidates old UI control scopes.
    if local_published {
        state.client_core_configuration_committed(active.as_ref());
    }
    let (app_history, rules_history) = result?;
    #[cfg(test)]
    {
        if let Some(run) =
            lock(&state.rule_migration().after_publish, "rule_migration_test")?.take()
        {
            run(state);
        }
    }
    let after = context(state, profile_id)?;
    verify_target(&dir, &after.rules, target_proof.as_ref())?;
    if after.binding.app_digest != next_app_digest
        || after.binding.rules_digest != next_rules_digest
        || after.binding.app_history != app_history
        || after.binding.rules_history != rules_history
        || after.binding.graph_digest != binding.graph_digest
        || after.binding.domain_epoch != binding.domain_epoch
        || after.binding.source_digest != binding.source_digest
        || after.binding.host_session_id != binding.host_session_id
        || after.binding.scope != binding.scope
        || after.binding.generation != binding.generation.saturating_add(1)
        || after.rules.iter().any(|r| r.id == resource_id)
    {
        return Err(conflict());
    }
    token(&after, profile_id, resource_id)
}

fn verify_target(
    dir: &std::path::Path,
    rules: &[RuleSetProfile],
    proof: Option<&(String, String)>,
) -> AppResult<()> {
    if let Some((id, expected)) = proof {
        let target = rules.iter().find(|r| &r.id == id).ok_or_else(conflict)?;
        if &crate::services::rule_migration_artifact::verify(dir, target)? != expected {
            return Err(conflict());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
