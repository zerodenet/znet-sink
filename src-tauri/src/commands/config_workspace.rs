use crate::{
    configuration::workspace::{self, Prepared},
    errors::{AppError, AppResult},
    kernel::{
        adapter::KernelAdapter,
        zero::{adapter::ZeroAdapter, queries},
    },
    models::{
        config_workspace::*,
        core::CoreIpcOptions,
        core_process::CoreProcessState,
        gui_core::{GuiConfigImpactItem, GuiConfigPlanApplyResult},
    },
    services::{common, core_config, core_process, interaction_mode, proxy_config},
    state::app_state::AppState,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tauri::{AppHandle, Manager, State, Window};
fn host(state: &AppState, window: &Window) -> AppResult<()> {
    crate::services::native_documents::require_host(window.label())?;
    interaction_mode::require_pro_mode(state, "config_workspace")
}
#[tauri::command]
pub async fn gui_config_workspace_snapshot(
    state: State<'_, AppState>,
    window: Window,
) -> AppResult<ConfigWorkspaceSnapshot> {
    host(state.inner(), &window)?;
    snapshot(state.inner()).await
}
#[tauri::command]
pub async fn gui_config_workspace_plan(
    state: State<'_, AppState>,
    window: Window,
    input: ConfigWorkspaceDraftInput,
) -> AppResult<ConfigWorkspacePlan> {
    host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    plan(state.inner(), input).await
}
#[tauri::command]
pub async fn gui_config_workspace_save_local(
    state: State<'_, AppState>,
    window: Window,
    input: ConfigWorkspaceDraftInput,
) -> AppResult<ConfigWorkspaceSaveResult> {
    host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    workspace::save(state.inner(), input)
}
#[tauri::command]
pub fn gui_config_workspace_validate(
    state: State<'_, AppState>,
    window: Window,
    input: ConfigWorkspaceDraftInput,
) -> AppResult<Value> {
    host(state.inner(), &window)?;
    let (_, source, effective) = workspace::from_input(state.inner(), &input)?;
    Ok(
        json!({"sourceJson":workspace::pretty(&source)?, "effectiveJson":workspace::pretty(&effective)?}),
    )
}
#[tauri::command]
pub fn gui_config_workspace_format(window: Window, source_json: String) -> AppResult<String> {
    crate::services::native_documents::require_host(window.label())?;
    workspace::pretty(&workspace::parse(&source_json)?)
}
#[tauri::command]
pub async fn gui_config_workspace_apply(
    app_handle: AppHandle,
    window: Window,
    input: ConfigWorkspaceApplyInput,
) -> AppResult<ConfigWorkspaceOperation> {
    let state = app_handle.state::<AppState>();
    host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    apply_locked(state.inner(), input).await
}
#[tauri::command]
pub async fn gui_config_workspace_operation(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<ConfigWorkspaceOperation> {
    host(state.inner(), &window)?;
    let mut operation = workspace::operation(state.inner(), &plan_id)?;
    // Only the original plan's receipt may be checked; equal digests do not grant authority.
    if let Some(receipt) = &operation.receipt {
        let identity = queries::runtime_identity(Some(ipc_options(state.inner())?)).await?;
        if identity.core_instance_id != receipt.runtime_identity.core_instance_id
            || identity.config_revision != receipt.runtime_identity.config_revision
        {
            operation.state = "submitted_unknown";
            operation.reason = Some("原回执后的运行身份已变化；不能将历史事务当作当前确认".into());
        }
    }
    Ok(operation)
}
#[tauri::command]
pub async fn gui_config_workspace_recovery_review(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<ConfigWorkspaceRecoveryReview> {
    host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    recovery_review_locked(state.inner(), &plan_id).await
}
async fn recovery_identity(
    state: &AppState,
    record: &workspace::recovery::Record,
) -> AppResult<()> {
    let receipt = record
        .meta
        .receipt
        .as_ref()
        .ok_or_else(workspace::conflict)?;
    let control = crate::kernel::configuration::BoundControl::connect(ipc_options(state)?).await?;
    let expected = queries::KernelRuntimeIdentity {
        core_instance_id: receipt.core_instance_id.clone(),
        config_revision: receipt.config_revision,
    };
    if control.identity().await? != expected || control.identity().await? != expected {
        return Err(workspace::conflict());
    }
    Ok(())
}
async fn recovery_review_locked(
    state: &AppState,
    plan_id: &str,
) -> AppResult<ConfigWorkspaceRecoveryReview> {
    // Admission before candidate body; a latest runtime query cannot manufacture a receipt.
    workspace::context(state)?;
    let metadata =
        workspace::recovery::metadata(state, plan_id)?.ok_or_else(workspace::conflict)?;
    let operation = workspace::recovery::view(&metadata);
    let mut result = ConfigWorkspaceRecoveryReview {
        plan_id: plan_id.into(),
        review_id: None,
        state: operation.state,
        can_complete_local: false,
        reason: operation.reason.unwrap_or_default(),
        receipt: operation.receipt,
        expires_at_unix_ms: None,
    };
    if !matches!(
        metadata.stage,
        workspace::recovery::Stage::RuntimeConfirmedPendingLocal
            | workspace::recovery::Stage::LocalCommitted
            | workspace::recovery::Stage::AppConfirmed
            | workspace::recovery::Stage::Complete
    ) {
        return Ok(result);
    }
    let record = workspace::recovery::load_checked(state, plan_id)?;
    recovery_identity(state, &record).await?;
    // Both local material and the original receipt are checked again after async reads.
    let latest = workspace::recovery::load_checked(state, plan_id)?;
    if latest.meta.version != record.meta.version {
        return Err(workspace::conflict());
    }
    let (id, expires) = workspace::recovery::issue_review(state, &latest)?;
    result.review_id = Some(id);
    result.expires_at_unix_ms = Some(expires.to_string());
    result.can_complete_local = true;
    result.reason =
        "原回执与当前运行身份及本地材料精确匹配；确认后只完成原候选的本地发布，不重放运行操作"
            .into();
    Ok(result)
}
#[tauri::command]
pub async fn gui_config_workspace_complete_local(
    state: State<'_, AppState>,
    window: Window,
    input: ConfigWorkspaceCompleteInput,
) -> AppResult<ConfigWorkspaceOperation> {
    host(state.inner(), &window)?;
    let _operation = state.proxy_config_operation().lock().await;
    complete_local_locked(state.inner(), input).await
}
async fn complete_local_locked(
    state: &AppState,
    input: ConfigWorkspaceCompleteInput,
) -> AppResult<ConfigWorkspaceOperation> {
    let review = workspace::recovery::consume_review(state, &input.plan_id, &input.review_id)?;
    let record = workspace::recovery::load_checked(state, &input.plan_id)?;
    if record.meta.version != review.version {
        return Err(workspace::conflict());
    }
    recovery_identity(state, &record).await?;
    workspace::verify(state, &review.binding)?;
    let result =
        workspace::recovery::finish_local(state, &input.plan_id, &review.binding, review.version)?;
    if let Some(receipt) = &result.receipt {
        state.configuration().record_transaction(receipt.clone());
    }
    Ok(result)
}
#[tauri::command]
pub fn gui_config_workspace_discard(
    state: State<'_, AppState>,
    window: Window,
    plan_id: String,
) -> AppResult<()> {
    host(state.inner(), &window)?;
    workspace::discard(state.inner(), &plan_id)
}
async fn snapshot(state: &AppState) -> AppResult<ConfigWorkspaceSnapshot> {
    let mut snapshot = workspace::local_snapshot(state)?;
    if snapshot.read_only_reason.is_some() {
        return Ok(snapshot);
    }
    let binding = snapshot.binding.clone();
    let running = core_process::refresh_status(state)?.state == CoreProcessState::Running;
    let identity = if running {
        queries::runtime_identity(Some(ipc_options(state)?))
            .await
            .ok()
            .map(Into::into)
    } else {
        None
    };
    if workspace::local_snapshot(state)?.binding != binding {
        return Err(workspace::conflict());
    }
    let confirmed = snapshot.last_transaction.as_ref().is_some_and(|r| {
        r.profile_id == snapshot.profile_id
            && r.effective_digest == snapshot.runtime.effective_digest
            && identity
                .as_ref()
                .is_some_and(|i: &ConfigRuntimeIdentityView| {
                    i.core_instance_id == r.runtime_identity.core_instance_id
                        && i.config_revision == r.runtime_identity.config_revision
                })
    });
    snapshot.runtime.running = Some(running);
    snapshot.runtime.identity = identity;
    snapshot.runtime.confirmed = confirmed;
    snapshot.pending_apply = !confirmed;
    snapshot.runtime.reason = if confirmed {
        "运行身份与已确认事务一致"
    } else if !running {
        "本地配置已保存，内核未运行；待应用。schema校验不代表实际运行兼容"
    } else {
        "本地来源与运行态尚未确认一致；待应用或核对"
    }
    .into();
    Ok(snapshot)
}
async fn plan(
    state: &AppState,
    input: ConfigWorkspaceDraftInput,
) -> AppResult<ConfigWorkspacePlan> {
    let (context, source, effective) = workspace::from_input(state, &input)?;
    let previous = crate::services::rule_overlay::compose_effective_candidate_with_app(
        state,
        context
            .profile
            .content
            .as_ref()
            .ok_or_else(workspace::conflict)?,
        Some(&context.profile.id),
        &context.app,
    )?;
    let impact = classify_impact(
        context
            .profile
            .content
            .as_ref()
            .ok_or_else(workspace::conflict)?,
        &source,
        &previous.config,
        &effective,
    );
    let capture = crate::configuration::capture_apply::candidate_capture_parameters(
        state,
        &source,
        Some(&context.profile.id),
    )?;
    let running = core_process::refresh_status(state)?.state == CoreProcessState::Running;
    let mut identity = None;
    let mut reason = if running {
        None
    } else {
        Some("内核未运行；本地原生schema/引用校验已通过，可保存本地待应用".into())
    };
    if !impact.requires_restart.is_empty() {
        reason = Some("此变更需要冻结启动材料和重启回执适配；当前工作区尚不能安全提交，保留来源并可保存本地。内核管理的普通重启仍独立可用".into());
    }
    if running && reason.is_none() {
        if crate::services::system_proxy_guard::is_enabled_by_guard()?
            && workspace::digest(
                &proxy_config::extract_local_proxy(&previous.config).map(|v| (v.host, v.port)),
            )? != workspace::digest(
                &proxy_config::extract_local_proxy(&effective).map(|v| (v.host, v.port)),
            )?
        {
            reason = Some(
                "当前系统代理绑定会受影响；请先明确关闭系统代理，再重新预检，不会隐式重定向".into(),
            );
        } else {
            let original = queries::runtime_identity(Some(ipc_options(state)?)).await?;
            let response = ZeroAdapter::new()
                .validate_config(effective.clone(), ipc_options(state)?)
                .await
                .map_err(|_| AppError::internal("运行内核预检未完成；草稿保留，未应用"))?;
            ensure_validation_accepted(&response)?;
            if queries::runtime_identity(Some(ipc_options(state)?)).await? != original {
                return Err(workspace::conflict());
            }
            identity = Some(original);
        }
    }
    workspace::verify(state, &context.binding)?;
    let plan = ConfigWorkspacePlan {
        impact,
        plan_id: String::new(),
        binding: workspace::digest(&context.binding)?,
        source_json: workspace::pretty(&source)?,
        effective_json: workspace::pretty(&effective)?,
        effective_digest: workspace::digest(&effective)?,
        expires_at_unix_ms: String::new(),
        runtime_validated: identity.is_some(),
        apply_unavailable_reason: reason,
    };
    workspace::store(
        state,
        Prepared {
            binding: context.binding,
            source,
            effective,
            capture_digest: workspace::digest(&capture)?,
            frozen_capture_parameters: capture,
            identity,
            confirmed_binding: None,
            plan,
            operation: ConfigWorkspaceOperation {
                plan_id: String::new(),
                state: "prepared",
                receipt: None,
                reason: None,
            },
        },
    )
}
async fn apply_locked(
    state: &AppState,
    input: ConfigWorkspaceApplyInput,
) -> AppResult<ConfigWorkspaceOperation> {
    if input.strategy == ConfigApplyStrategy::Restart {
        return Err(AppError::invalid_argument("工作区重启暂缺冻结启动材料及回执适配；草稿保留，可保存本地待应用，不会执行旧的非CAS恢复"));
    }
    let original = workspace::operation(state, &input.plan_id)?;
    if original.state != "prepared" {
        return Err(AppError {
            code: if original.state == "rejected" {
                "workspace_preflight_rejected"
            } else {
                "config_apply_uncertain"
            },
            message: "原计划已结束或已提交；只能核对原操作，不能再次运行变更".into(),
            details: None,
        });
    }
    let preflight = async {
        let prepared = workspace::review(state, &input.plan_id)?;
        let expected = prepared.identity.as_ref().ok_or_else(workspace::conflict)?;
        if &queries::runtime_identity(Some(ipc_options(state)?)).await? != expected {
            return Err(workspace::conflict());
        }
        workspace::verify(state, &prepared.binding)?;
        Ok::<_, AppError>(prepared)
    }
    .await;
    if preflight.is_err() {
        workspace::reject_before_submit(state, &input.plan_id)?;
        return Err(AppError {
            code: "workspace_preflight_rejected",
            message: "副作用前预检未通过；未发送运行变更，草稿保留，请重新预检".into(),
            details: None,
        });
    }
    let prepared = workspace::submit(state, &input.plan_id)?;
    let result = async {
        let context = workspace::verify(state, &prepared.binding)?;
        let started = common::now_unix_ms();
        let receipt = crate::configuration::capture_apply::apply_profile_expected(
            state,
            &prepared.source,
            Some(&context.profile.id),
            prepared.effective.clone(),
            ipc_options(state)?,
            prepared.identity.as_ref().ok_or_else(workspace::conflict)?,
            &prepared.frozen_capture_parameters,
        )
        .await?;
        if workspace::digest(receipt.applied_config())? != prepared.plan.effective_digest
            || workspace::digest(receipt.applied_capture_parameters())? != prepared.capture_digest
        {
            return Err(workspace::conflict());
        }
        let transaction = ConfigTransactionReceipt {
            transaction_id: state.configuration().next_transaction_id(),
            profile_id: context.profile.id.clone(),
            strategy: input.strategy,
            state: "runtime_confirmed",
            effective_digest: prepared.plan.effective_digest,
            started_at_unix_ms: started,
            completed_at_unix_ms: common::now_unix_ms(),
            runtime_identity: receipt.identity.into(),
        };
        workspace::runtime_confirmed(state, &input.plan_id, transaction.clone())?;
        workspace::verify(state, &prepared.binding)?;
        let record = workspace::recovery::load_checked(state, &input.plan_id)?;
        let operation = workspace::recovery::finish_local(
            state,
            &input.plan_id,
            &prepared.binding,
            record.meta.version,
        )?;
        if let Some(receipt) = &operation.receipt {
            state.configuration().record_transaction(receipt.clone());
        }
        Ok(operation)
    }
    .await;
    if result
        .as_ref()
        .is_err_and(|error| error.code == "capture_preflight_rejected")
    {
        workspace::reject_without_runtime_mutation(state, &input.plan_id)?;
        return Err(AppError {
            code: "workspace_preflight_rejected",
            message: "冻结接管参数在副作用前已变化；未发送运行变更，草稿保留，请重新预检".into(),
            details: None,
        });
    }
    result.map_err(|_| AppError {
        code: "config_apply_uncertain",
        message: "原工作区事务未完整确认；请核对原操作的运行回执与本地阶段，不能重放或声称已回滚"
            .into(),
        details: Some(json!({"planId":input.plan_id})),
    })
}

fn ipc_options(state: &AppState) -> AppResult<CoreIpcOptions> {
    Ok(core_config::ipc_options_from_app_config(
        &common::lock(state.app_config(), "app_config")?.core,
    ))
}
fn ensure_validation_accepted(response: &Value) -> AppResult<()> {
    let mut current = response;
    for _ in 0..6 {
        if current.get("valid").and_then(Value::as_bool) == Some(true) {
            return Ok(());
        }
        if current.get("valid").and_then(Value::as_bool) == Some(false)
            || current.get("ok").and_then(Value::as_bool) == Some(false)
        {
            return Err(AppError::invalid_argument(
                "运行内核未接受配置校验；原草稿保留",
            ));
        }
        if let Some(result) = current.get("result") {
            current = result;
        } else {
            break;
        }
    }
    Err(AppError::internal("运行内核未提供可识别的校验回执"))
}
impl From<queries::KernelRuntimeIdentity> for ConfigRuntimeIdentityView {
    fn from(value: queries::KernelRuntimeIdentity) -> Self {
        Self {
            core_instance_id: value.core_instance_id,
            config_revision: value.config_revision,
        }
    }
}
fn classify_impact(
    previous_source: &Value,
    next_source: &Value,
    previous_effective: &Value,
    next_effective: &Value,
) -> GuiConfigPlanApplyResult {
    let mut hot_reload = Vec::new();
    let mut requires_restart = Vec::new();
    let mut sections = BTreeSet::new();
    for value in [previous_effective, next_effective] {
        if let Some(object) = value.as_object() {
            sections.extend(object.keys().cloned());
        }
    }
    for section in sections {
        if previous_effective.get(&section) == next_effective.get(&section) {
            continue;
        }
        if section == "runtime" {
            classify_runtime(
                previous_effective.get(&section),
                next_effective.get(&section),
                &mut hot_reload,
                &mut requires_restart,
            );
            continue;
        }
        let item = impact_item(
            &section,
            previous_effective.get(&section),
            next_effective.get(&section),
        );
        if is_restart_section(&section) {
            requires_restart.push(item);
        } else {
            hot_reload.push(item);
        }
    }
    if previous_source.pointer("/runtime/tun") != next_source.pointer("/runtime/tun")
        && !hot_reload.iter().any(|item| item.section == "runtime.tun")
    {
        hot_reload.push(impact_item(
            "runtime.tun",
            previous_source.pointer("/runtime/tun"),
            next_source.pointer("/runtime/tun"),
        ));
    }
    GuiConfigPlanApplyResult {
        valid: true,
        hot_reload,
        requires_restart,
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

fn is_restart_section(section: &str) -> bool {
    !matches!(
        section,
        "outbounds"
            | "outbound_groups"
            | "endpoints"
            | "route"
            | "dns"
            | "log"
            | "event_sinks"
            | "extensions"
    )
}

fn classify_runtime(
    before: Option<&Value>,
    after: Option<&Value>,
    hot_reload: &mut Vec<GuiConfigImpactItem>,
    requires_restart: &mut Vec<GuiConfigImpactItem>,
) {
    let restart_keys = ["api", "ipc", "principal_quota_state_path"];
    for key in restart_keys {
        let old = before.and_then(|value| value.get(key));
        let new = after.and_then(|value| value.get(key));
        if old != new {
            requires_restart.push(impact_item(&format!("runtime.{key}"), old, new));
        }
    }

    let mut old_hot = before.cloned().unwrap_or_else(|| json!({}));
    let mut new_hot = after.cloned().unwrap_or_else(|| json!({}));
    for value in [&mut old_hot, &mut new_hot] {
        if let Some(object) = value.as_object_mut() {
            for key in restart_keys {
                object.remove(key);
            }
        }
    }
    if old_hot != new_hot {
        hot_reload.push(impact_item("runtime", Some(&old_hot), Some(&new_hot)));
    }
}

fn impact_item(
    section: &str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> GuiConfigImpactItem {
    let tags = before
        .into_iter()
        .chain(after)
        .flat_map(extract_tags)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let detail = match (before, after) {
        (None, Some(_)) => "新增配置段",
        (Some(_), None) => "移除配置段",
        _ => "修改配置段",
    };
    GuiConfigImpactItem {
        section: section.to_string(),
        tags,
        detail: detail.to_string(),
    }
}

fn extract_tags(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("tag")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::classify_impact;

    #[test]
    fn classifies_listener_restart_separately_from_automatic_tun_transition() {
        let before = json!({"inbounds":[{"tag":"mixed"}],"route":{"rules":[]},"runtime":{"tun":{"mtu":1500}}});
        let source_after = json!({"inbounds":[{"tag":"socks"}],"route":{"rules":[1]},"runtime":{"tun":{"mtu":1400}}});
        let effective_before =
            json!({"inbounds":[{"tag":"mixed"}],"route":{"rules":[]},"runtime":{}});
        let effective_after =
            json!({"inbounds":[{"tag":"socks"}],"route":{"rules":[1]},"runtime":{}});
        let plan = classify_impact(&before, &source_after, &effective_before, &effective_after);
        assert!(plan
            .requires_restart
            .iter()
            .any(|item| item.section == "inbounds"));
        assert!(plan
            .hot_reload
            .iter()
            .any(|item| item.section == "runtime.tun"));
        assert!(plan.hot_reload.iter().any(|item| item.section == "route"));
    }

    #[test]
    fn splits_runtime_hot_fields_from_process_bound_fields() {
        let before = json!({"runtime":{"latency_test_url":"https://a","api":{"listen":"a"}}});
        let after = json!({"runtime":{"latency_test_url":"https://b","api":{"listen":"b"}}});
        let plan = classify_impact(&before, &after, &before, &after);
        assert!(plan.hot_reload.iter().any(|item| item.section == "runtime"));
        assert!(plan
            .requires_restart
            .iter()
            .any(|item| item.section == "runtime.api"));
    }

    #[test]
    fn treats_unknown_top_level_contracts_conservatively() {
        let before = json!({"future_contract":{"enabled":false}});
        let after = json!({"future_contract":{"enabled":true}});
        let plan = classify_impact(&before, &after, &before, &after);
        assert!(plan.hot_reload.is_empty());
        assert_eq!(plan.requires_restart[0].section, "future_contract");
    }

    #[test]
    fn native_principal_quota_state_path_is_startup_only() {
        let before = json!({"runtime":{"principal_quota_state_path":"before"}});
        let after = json!({"runtime":{"principal_quota_state_path":"after"}});
        let plan = classify_impact(&before, &after, &before, &after);
        assert!(plan.hot_reload.is_empty());
        assert_eq!(
            plan.requires_restart[0].section,
            "runtime.principal_quota_state_path"
        );
    }

    #[test]
    fn endpoint_changes_are_managed_reload_operations() {
        let before = json!({"endpoints":[{"tag":"a","enabled":true}]});
        let after = json!({"endpoints":[{"tag":"a","enabled":false}]});
        let plan = classify_impact(&before, &after, &before, &after);
        assert!(plan.requires_restart.is_empty());
        assert_eq!(plan.hot_reload[0].section, "endpoints");
    }
}

#[cfg(all(test, unix))]
mod ipc_tests;

#[cfg(all(test, unix))]
#[path = "config_workspace/recovery_tests.rs"]
mod recovery_tests;
