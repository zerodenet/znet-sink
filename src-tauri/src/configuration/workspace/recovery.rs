//! Receipt-bound local completion. Never dispatches a runtime command or capture change.
use super::*;
use crate::services::{
    app_config_store,
    app_database::{self, workspace_recovery as db},
};
pub(crate) use db::{Record, Stage};
use std::path::PathBuf;
pub(crate) fn directory(state: &AppState) -> AppResult<PathBuf> {
    if let Some(dir) = lock(&state.configuration().plans.1, "workspace_directory")?.clone() {
        return Ok(dir);
    }
    crate::services::data_dir()
}
pub(crate) fn metadata(state: &AppState, id: &str) -> AppResult<Option<db::Metadata>> {
    db::load_meta(&directory(state)?, id)
}
pub(crate) fn pending(state: &AppState, profile: &str) -> AppResult<Option<db::Metadata>> {
    db::pending(&directory(state)?, profile)
}
pub(crate) fn view(meta: &db::Metadata) -> ConfigWorkspaceOperation {
    let mut reason = match meta.stage {
        Stage::Prepared => "仅有持久准备记录，未取得提交阶段证明；结果未确认，不会重放",
        Stage::SubmittedUnknown => {
            "原操作提交阶段已持久记录，运行结果未确认；不会重放或从当前revision推断成功"
        }
        Stage::RuntimeConfirmedPendingLocal | Stage::LocalCommitted | Stage::AppConfirmed => {
            "原运行回执已持久确认，本地发布未完成；需显式核对后仅完成原本地候选"
        }
        Stage::Complete => "原操作本地发布已完成；仍需核对当前来源和原运行身份",
        Stage::Rejected => "副作用前已明确拒绝，不能重复原计划",
    }
    .to_string();
    if meta.receipt.as_ref().is_some_and(|r| r.clock_regressed) {
        reason.push_str("；记录期间系统时钟回退，原始时间仅作历史资料，不用于授予恢复权限");
    }
    ConfigWorkspaceOperation {
        plan_id: meta.operation_id.clone(),
        state: meta.stage.wire(),
        receipt: meta.receipt.as_ref().map(|r| r.view(meta)),
        reason: Some(reason),
    }
}
pub(crate) fn prepare_submitted(
    state: &AppState,
    p: &Prepared,
    context: &Context,
) -> AppResult<()> {
    let expected = p.identity.as_ref().ok_or_else(conflict)?;
    let mut candidate = context.profile.clone();
    candidate.content = Some(p.source.clone());
    candidate.updated_at_unix_ms =
        now_unix_ms().max(candidate.updated_at_unix_ms.saturating_add(1));
    candidate.capabilities = proxy_config::analyze_capabilities(candidate.content.as_ref());
    let mut after = context.profiles.clone();
    *after
        .iter_mut()
        .find(|x| x.id == candidate.id)
        .ok_or_else(conflict)? = candidate.clone();
    let meta = db::Metadata {
        schema: 1,
        operation_id: p.plan.plan_id.clone(),
        profile_id: candidate.id.clone(),
        session: p.binding.host_session_id.clone(),
        version: 1,
        stage: Stage::Prepared,
        epoch: p.binding.domain_epoch,
        created_at_unix_ms: now_unix_ms(),
        before_graph_digest: p.binding.graph_digest.clone(),
        after_graph_digest: digest(&(&after, &context.subscriptions))?,
        source_digest: p.binding.source_digest.clone(),
        candidate_digest: digest(&candidate)?,
        app_digest: p.binding.app_digest.clone(),
        rules_digest: p.binding.rules_digest.clone(),
        effective_digest: p.plan.effective_digest.clone(),
        capture_digest: p.capture_digest.clone(),
        original_core_instance_id: expected.core_instance_id.clone(),
        original_config_revision: expected.config_revision,
        original_scope_digest: digest(&p.binding.scope)?,
        original_generation: p.binding.generation,
        receipt: None,
    };
    let dir = directory(state)?;
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    if digest(&(&*profiles, &*subs))? != p.binding.graph_digest
        || digest(&*rules)? != p.binding.rules_digest
        || digest(&*app)? != p.binding.app_digest
    {
        return Err(conflict());
    }
    state.with_client_scope_generation(&p.binding.scope, p.binding.generation, || {
        if state.host_session_id()? != p.binding.host_session_id {
            return Err(conflict());
        }
        let r = db::prepare(
            &dir,
            Record {
                meta,
                candidate: Some(candidate),
            },
        )?;
        db::submitted(&dir, &r.meta.operation_id, r.meta.version)?;
        Ok(())
    })?;
    Ok(())
}
pub(crate) fn rejected(state: &AppState, id: &str) -> AppResult<()> {
    if let Some(m) = metadata(state, id)? {
        db::rejected(&directory(state)?, id, m.version)?;
    }
    Ok(())
}
pub(crate) fn confirmed(
    state: &AppState,
    id: &str,
    receipt: &ConfigTransactionReceipt,
) -> AppResult<()> {
    let m = metadata(state, id)?.ok_or_else(conflict)?;
    if receipt.profile_id != m.profile_id
        || receipt.effective_digest != m.effective_digest
        || receipt.state != "runtime_confirmed"
    {
        return Err(conflict());
    }
    db::confirm(
        &directory(state)?,
        id,
        m.version,
        db::Evidence {
            transaction_id: receipt.transaction_id,
            core_instance_id: receipt.runtime_identity.core_instance_id.clone(),
            config_revision: receipt.runtime_identity.config_revision,
            started_at_unix_ms: receipt.started_at_unix_ms,
            completed_at_unix_ms: receipt.completed_at_unix_ms,
            clock_regressed: receipt.started_at_unix_ms < m.created_at_unix_ms
                || receipt.completed_at_unix_ms < receipt.started_at_unix_ms,
        },
    )?;
    Ok(())
}
fn expected_graph(m: &db::Metadata) -> &str {
    if matches!(
        m.stage,
        Stage::LocalCommitted | Stage::AppConfirmed | Stage::Complete
    ) {
        &m.after_graph_digest
    } else {
        &m.before_graph_digest
    }
}
/// A DB commit may have succeeded before delivery to memory. Promote only the
/// two exact graphs owned by this record, never arbitrary new relations.
pub(crate) fn reconcile(state: &AppState, record: &Record) -> AppResult<()> {
    let dir = directory(state)?;
    let scope = state.client_core_snapshot();
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let mut subs = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    if digest(&*rules)? != record.meta.rules_digest || digest(&*app)? != record.meta.app_digest {
        return Err(db::changed());
    }
    let memory = digest(&(&*profiles, &*subs))?;
    if memory != record.meta.before_graph_digest && memory != record.meta.after_graph_digest {
        return Err(db::changed());
    }
    let changed = memory != expected_graph(&record.meta);
    let active =
        state.with_client_scope_generation(&scope.scope, scope.scope_generation.0, || {
            db::reconcile_graph(&dir, &record.meta, |actual| {
                if changed {
                    *profiles = actual.proxy_configs;
                    *subs = actual.subscriptions;
                }
                profiles.iter().find(|p| p.active).cloned()
            })
        })?;
    drop(app);
    drop(rules);
    drop(subs);
    drop(profiles);
    if changed {
        state.client_core_configuration_committed(active.as_ref());
    }
    Ok(())
}
pub(crate) fn load_checked(state: &AppState, id: &str) -> AppResult<Record> {
    let current = context(state)?;
    let meta = metadata(state, id)?.ok_or_else(conflict)?;
    if meta.profile_id != current.profile.id {
        return Err(conflict());
    }
    let record = db::load(&directory(state)?, id)?.ok_or_else(conflict)?;
    reconcile(state, &record)?;
    let current = context(state)?;
    if current.binding.graph_digest != expected_graph(&record.meta)
        || current.binding.app_digest != record.meta.app_digest
        || current.binding.rules_digest != record.meta.rules_digest
        || db::current_epoch(&directory(state)?)? != record.meta.epoch
    {
        return Err(db::changed());
    }
    if !matches!(
        record.meta.stage,
        Stage::Prepared | Stage::SubmittedUnknown | Stage::Rejected
    ) {
        let source = record
            .candidate
            .as_ref()
            .and_then(|p| p.content.as_ref())
            .or_else(|| current.profile.content.as_ref())
            .ok_or_else(conflict)?;
        let effective = rule_overlay::compose_effective_candidate_with_app(
            state,
            source,
            Some(&current.profile.id),
            &current.app,
        )?;
        let capture = crate::configuration::capture_apply::candidate_capture_parameters(
            state,
            source,
            Some(&current.profile.id),
        )?;
        if digest(&effective.config)? != record.meta.effective_digest
            || digest(&capture)? != record.meta.capture_digest
            || record.meta.receipt.is_none()
        {
            return Err(conflict());
        }
        verify(state, &current.binding)?;
    }
    Ok(record)
}
#[derive(Clone)]
pub(crate) struct Review {
    pub operation_id: String,
    pub version: i64,
    pub binding: Binding,
    pub deadline: std::time::Instant,
}
pub(crate) fn issue_review(state: &AppState, record: &Record) -> AppResult<(String, u64)> {
    let mut reviews = lock(&state.configuration().plans.2, "workspace_recovery_reviews")?;
    let now = now_unix_ms();
    reviews.retain(|_, r| r.deadline > std::time::Instant::now());
    if reviews.len() >= 4 {
        return Err(AppError::invalid_argument(
            "已有四个恢复核对，请关闭或等待过期",
        ));
    }
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| AppError::internal("无法建立恢复核对"))?;
    let id = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let expires = now.saturating_add(300_000);
    reviews.insert(
        id.clone(),
        Review {
            operation_id: record.meta.operation_id.clone(),
            version: record.meta.version,
            binding: context(state)?.binding,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(300),
        },
    );
    Ok((id, expires))
}
pub(crate) fn consume_review(state: &AppState, id: &str, review_id: &str) -> AppResult<Review> {
    let review = lock(&state.configuration().plans.2, "workspace_recovery_reviews")?
        .remove(review_id)
        .ok_or_else(conflict)?;
    if review.operation_id != id || review.deadline <= std::time::Instant::now() {
        return Err(conflict());
    }
    verify(state, &review.binding)?;
    Ok(review)
}
/// Caller holds proxy_config_operation and has freshly checked the original
/// persisted receipt on a single BoundControl. All work below is local only.
pub(crate) fn finish_local(
    state: &AppState,
    id: &str,
    binding: &Binding,
    version: i64,
) -> AppResult<ConfigWorkspaceOperation> {
    let dir = directory(state)?;
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    let mut r = db::load(&dir, id)?.ok_or_else(conflict)?;
    if r.meta.version != version
        || r.meta.receipt.is_none()
        || !matches!(
            r.meta.stage,
            Stage::RuntimeConfirmedPendingLocal
                | Stage::LocalCommitted
                | Stage::AppConfirmed
                | Stage::Complete
        )
        || digest(&*app)? != r.meta.app_digest
        || digest(&*rules)? != r.meta.rules_digest
        || digest(&(&*profiles, &*subs))? != expected_graph(&r.meta)
    {
        return Err(conflict());
    }
    let was_published = r.meta.stage == Stage::RuntimeConfirmedPendingLocal;
    let active = state.with_client_scope_generation(&binding.scope, binding.generation, || {
        if binding.host_session_id != state.host_session_id()?
            || db::current_epoch(&dir)? != r.meta.epoch
        {
            return Err(conflict());
        }
        let app_path = dir.join("app-config.json");
        if app_path.is_file() {
            let bytes = std::fs::read(&app_path).map_err(|_| db::changed())?;
            if bytes.len() > 16 * 1024 * 1024 {
                return Err(db::changed());
            }
            let persisted: AppConfig = serde_json::from_slice(&bytes).map_err(|_| db::changed())?;
            if digest(&persisted)? != r.meta.app_digest {
                return Err(db::changed());
            }
        }
        if r.meta.stage == Stage::RuntimeConfirmedPendingLocal {
            r = db::publish(&dir, id, r.meta.version)?;
        }
        if r.meta.stage == Stage::LocalCommitted {
            // The workspace preserves AppConfig and local-proxy ownership. Save
            // the exact held value atomically, then checkpoint this separate file.
            app_config_store::save(&dir.join("app-config.json"), &app)?;
            r = db::app_confirmed(&dir, id, r.meta.version)?;
        }
        if r.meta.stage == Stage::AppConfirmed {
            r = db::complete(&dir, id, r.meta.version)?;
        }
        if was_published {
            let actual = app_database::load_domain_data(&dir)?;
            if digest(&(&actual.proxy_configs, &actual.subscriptions))? != r.meta.after_graph_digest
            {
                return Err(conflict());
            }
            *profiles = actual.proxy_configs;
        }
        Ok(profiles.iter().find(|p| p.active).cloned())
    })?;
    drop(app);
    drop(rules);
    drop(subs);
    drop(profiles);
    if was_published {
        state.client_core_configuration_committed(active.as_ref());
    }
    // Retire only this exact completed operation. Other unknown plans retain
    // their barriers; the durable tombstone remains available by original ID.
    if r.meta.stage == Stage::Complete {
        lock(&state.configuration().plans.0, "workspace_plans")?.remove(id);
    }
    Ok(view(&r.meta))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn set_test_directory(state: &AppState, dir: PathBuf) {
    *state.configuration().plans.1.lock().unwrap() = Some(dir);
}

/// A tombstone may restore the original receipt to the read-only snapshot, but
/// only if the complete current material and persistent epoch still match.
pub(crate) fn completed_transaction(
    state: &AppState,
    context: &Context,
    effective: &Value,
) -> AppResult<Option<ConfigTransactionReceipt>> {
    let Some(meta) = db::latest(&directory(state)?, &context.profile.id)? else {
        return Ok(None);
    };
    if meta.stage != Stage::Complete
        || meta.epoch != context.binding.domain_epoch
        || meta.after_graph_digest != context.binding.graph_digest
        || meta.app_digest != context.binding.app_digest
        || meta.rules_digest != context.binding.rules_digest
        || meta.candidate_digest != context.binding.source_digest
        || meta.effective_digest != digest(effective)?
    {
        return Ok(None);
    }
    Ok(meta.receipt.as_ref().map(|r| r.view(&meta)))
}
