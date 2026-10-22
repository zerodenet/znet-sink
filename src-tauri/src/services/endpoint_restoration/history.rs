//! Fixed cooperative file generations, independent of hashes and UI tickets.
use super::*;
use crate::services::{
    app_config_store, domain_store,
    file_history::{self, Domain, HistoryToken, WriteReceipt},
};
use std::path::Path;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Files {
    pub app: HistoryToken,
    pub rules: HistoryToken,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Owner {
    pub host: String,
    pub generation: u64,
}
pub(super) fn owner(state: &AppState) -> AppResult<Owner> {
    Ok(Owner {
        host: state.host_session_id()?.into(),
        generation: state.client_core_snapshot().scope_generation.0,
    })
}
pub(super) fn require_owner(state: &AppState, record: &Record) -> AppResult<()> {
    let owner = record
        .owner
        .as_ref()
        .ok_or_else(|| conflict("旧恢复票据缺少宿主与文件历史绑定，仅保留历史；请重新预检"))?;
    if owner.host != state.host_session_id()? {
        return Err(conflict("恢复票据属于旧宿主；不能恢复新副作用权限"));
    }
    state.ensure_client_scope_generation(&record.plan.client_scope, owner.generation)
}
fn app_path(dir: &Path) -> PathBuf {
    dir.join("app-config.json")
}
fn rule_path(dir: &Path) -> PathBuf {
    dir.join("rule-sets.json")
}
fn files(record: &Record) -> AppResult<&Files> {
    record
        .binding
        .files
        .as_ref()
        .ok_or_else(|| conflict("旧恢复记录没有可信文件历史；保留原记录且不重放"))
}
fn compare_actual(
    app: &AppConfig,
    rules: &[crate::models::rule_set::RuleSetProfile],
    app_guard: &file_history::Guard,
    rule_guard: &file_history::Guard,
) -> AppResult<()> {
    if app_config_store::read_current_locked(app_guard)? != *app
        || digest(&domain_store::read_rule_sets_locked(rule_guard)?)? != digest(&rules)?
    {
        return Err(conflict(
            "实际设置或规则文件与客户端状态不一致，请显式刷新后重新预检",
        ));
    }
    Ok(())
}
pub(super) fn capture(state: &AppState, expected: &AppConfig) -> AppResult<Files> {
    capture_at(state, expected, &crate::services::data_dir()?)
}
fn capture_at(state: &AppState, expected: &AppConfig, dir: &Path) -> AppResult<Files> {
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    if &*app != expected {
        return Err(conflict("设置已变化，请重新预检"));
    }
    let app_guard = file_history::acquire(&app_path(dir), Domain::AppConfig)?;
    let rule_guard = file_history::acquire(&rule_path(dir), Domain::RuleSets)?;
    compare_actual(&app, &rules, &app_guard, &rule_guard)?;
    Ok(Files {
        app: app_guard.token()?,
        rules: rule_guard.token()?,
    })
}
pub(super) fn require(state: &AppState, record: &Record, expected: &AppConfig) -> AppResult<()> {
    require_at(
        state,
        files(record)?,
        expected,
        &crate::services::data_dir()?,
    )
}
pub(super) fn require_binding(
    state: &AppState,
    binding: &Binding,
    expected: &AppConfig,
) -> AppResult<()> {
    require_at(
        state,
        binding
            .files
            .as_ref()
            .ok_or_else(|| conflict("旧来源绑定缺少文件历史，请重新刷新端点"))?,
        expected,
        &crate::services::data_dir()?,
    )
}
fn require_at(
    state: &AppState,
    expected_files: &Files,
    expected: &AppConfig,
    dir: &Path,
) -> AppResult<()> {
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    if &*app != expected {
        return Err(conflict("客户端设置已变化，请重新预检"));
    }
    let app_guard = file_history::acquire(&app_path(dir), Domain::AppConfig)?;
    let rule_guard = file_history::acquire(&rule_path(dir), Domain::RuleSets)?;
    app_guard.require(&expected_files.app)?;
    rule_guard.require(&expected_files.rules)?;
    compare_actual(&app, &rules, &app_guard, &rule_guard)
}
pub(super) fn publish(
    state: &AppState,
    record: &Record,
    expected: &AppConfig,
    next: AppConfig,
) -> AppResult<WriteReceipt> {
    publish_at(state, record, expected, next, &crate::services::data_dir()?)
}
fn publish_at(
    state: &AppState,
    record: &Record,
    expected: &AppConfig,
    next: AppConfig,
    dir: &Path,
) -> AppResult<WriteReceipt> {
    let files = files(record)?;
    let owner = record
        .owner
        .as_ref()
        .ok_or_else(|| conflict("旧恢复票据没有本地发布资格"))?;
    if owner.host != state.host_session_id()? {
        return Err(conflict("本地发布属于旧宿主"));
    }
    if !record.local_write_started {
        return Err(conflict("原本地保存意图尚未持久记录，不允许写入"));
    }
    let rules = lock(state.rule_sets(), "rule_set")?;
    state.with_client_scope_generation(&record.plan.client_scope, owner.generation, || {
        let mut app = lock(state.app_config(), "app_config")?;
        if &*app != expected {
            return Err(conflict("客户端设置已变化，未写入旧候选"));
        }
        let mut app_guard = file_history::acquire(&app_path(dir), Domain::AppConfig)?;
        let rule_guard = file_history::acquire(&rule_path(dir), Domain::RuleSets)?;
        app_guard.require(&files.app)?;
        rule_guard.require(&files.rules)?;
        compare_actual(&app, &rules, &app_guard, &rule_guard)?;
        let receipt =
            app_config_store::save_owned_locked(&mut app_guard, &next, &record.plan.plan_id)?;
        *app = next;
        Ok(receipt)
    })
}

fn matches_origin(receipt: &WriteReceipt, before: &HistoryToken, owner: &str) -> bool {
    receipt.owner_operation.as_deref() == Some(owner)
        && receipt.before == *before
        && receipt.after.store_id == before.store_id
        && receipt.after.file_id == before.file_id
        && before.epoch.checked_add(1) == Some(receipt.after.epoch)
}
pub(super) fn published_matches(record: &Record, current: &Files) -> bool {
    let Some(before) = &record.binding.files else {
        return false;
    };
    record.app_write.as_ref().is_some_and(|receipt| {
        matches_origin(receipt, &before.app, &record.plan.plan_id)
            && receipt.after == current.app
            && before.rules == current.rules
    })
}
/// Only actual expected typed bytes plus the original history owner can finish
/// a previous file write. This path never writes AppConfig or invokes core.
pub(super) fn confirm_local(
    state: &AppState,
    record: &Record,
    source: &Value,
) -> AppResult<WriteReceipt> {
    confirm_local_at(state, record, source, &crate::services::data_dir()?)
}
fn confirm_local_at(
    state: &AppState,
    record: &Record,
    source: &Value,
    dir: &Path,
) -> AppResult<WriteReceipt> {
    let files = files(record)?;
    if !record.local_write_started || (!record.save_only && !record.receipt.runtime_confirmed) {
        return Err(conflict("没有原保存意图与已确认回执，不能采纳文件写入事实"));
    }
    if record
        .app_write
        .as_ref()
        .is_some_and(|receipt| !matches_origin(receipt, &files.app, &record.plan.plan_id))
    {
        return Err(conflict("原文件回执身份或历史代次无效，不能核对元数据"));
    }
    let current_scope = state.client_core_snapshot();
    let rules = lock(state.rule_sets(), "rule_set")?;
    state.with_client_scope_generation(
        &current_scope.scope,
        current_scope.scope_generation.0,
        || {
            let mut app = lock(state.app_config(), "app_config")?;
            let mut previous = app.clone();
            if overlay(
                &previous,
                &record.binding.target.profile_id,
                &record.binding.target.tag,
            )
            .is_none()
            {
                previous
                    .profile_edits
                    .entry(record.binding.target.profile_id.clone())
                    .or_default()
                    .entry("endpoints".into())
                    .or_insert_with(|| json!({}))[&record.binding.target.tag] =
                    record.overlay.clone();
            }
            if digest(&previous)? != record.app_digest {
                return Err(conflict("设置或覆盖已变化，不能用旧操作确认本地文件"));
            }
            let next = endpoints::restore_candidate(
                &previous,
                &record.binding.target.profile_id,
                source,
                &record.binding.snapshot,
            )?;
            let mut app_guard = file_history::acquire(&app_path(dir), Domain::AppConfig)?;
            let rule_guard = file_history::acquire(&rule_path(dir), Domain::RuleSets)?;
            rule_guard.require(&files.rules)?;
            if digest(&domain_store::read_rule_sets_locked(&rule_guard)?)? != digest(&*rules)?
                || app_config_store::read_current_locked(&app_guard)? != next
            {
                return Err(conflict("实际文件不是原操作的精确候选；不会重写或清除屏障"));
            }
            let after = app_guard.confirm_owned(&record.plan.plan_id, &files.app)?;
            if record.app_write.as_ref().is_some_and(|receipt| {
                receipt.before != files.app
                    || receipt.after != after
                    || receipt.owner_operation.as_deref() != Some(&record.plan.plan_id)
            }) {
                return Err(conflict(
                    "原文件写入回执不匹配，不采用其他操作或后来的相同值",
                ));
            }
            *app = next;
            Ok(WriteReceipt {
                before: files.app.clone(),
                after,
                owner_operation: Some(record.plan.plan_id.clone()),
            })
        },
    )
}
#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
