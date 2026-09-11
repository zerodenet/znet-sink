use std::collections::BTreeSet;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::kernel::{
    adapter::KernelAdapter,
    zero::{adapter::ZeroAdapter, queries},
};
use crate::models::config_workspace::{
    ConfigApplyStrategy, ConfigRuntimeIdentityView, ConfigTransactionReceipt,
    ConfigWorkspaceApplyInput, ConfigWorkspacePlan, ConfigWorkspaceRuntimeView,
    ConfigWorkspaceSnapshot,
};
use crate::models::core::CoreIpcOptions;
use crate::models::core_process::CoreProcessState;
use crate::models::gui_core::{GuiConfigImpactItem, GuiConfigPlanApplyResult};
use crate::models::proxy_config::ProxyConfigProfile;
use crate::services::{
    common, core_config, core_process, interaction_mode, proxy_config, rule_overlay,
};
use crate::state::app_state::AppState;

#[tauri::command]
pub async fn gui_config_workspace_snapshot(
    state: State<'_, AppState>,
) -> AppResult<ConfigWorkspaceSnapshot> {
    interaction_mode::require_pro_mode(state.inner(), "config_workspace")?;
    snapshot(state.inner()).await
}

#[tauri::command]
pub async fn gui_config_workspace_plan(
    state: State<'_, AppState>,
    source_config: Value,
) -> AppResult<ConfigWorkspacePlan> {
    interaction_mode::require_pro_mode(state.inner(), "config_workspace")?;
    let active = active_profile(state.inner())?;
    plan(state.inner(), &active, source_config, true).await
}

#[tauri::command]
pub async fn gui_config_workspace_apply(
    app_handle: AppHandle,
    input: ConfigWorkspaceApplyInput,
) -> AppResult<ConfigTransactionReceipt> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "config_workspace")?;
    let _operation = state.proxy_config_operation().lock().await;
    apply_locked(app_handle.clone(), state.clone(), input).await
}

async fn snapshot(state: &AppState) -> AppResult<ConfigWorkspaceSnapshot> {
    let active = active_profile(state)?;
    let source = active
        .content
        .clone()
        .ok_or_else(|| AppError::invalid_argument("当前活动配置没有可编辑的 JSON 内容"))?;
    let candidate =
        rule_overlay::compose_effective_candidate_for(state, &source, Some(&active.id))?;
    let digest = digest(&candidate.config)?;
    let last_transaction = state.configuration().transaction();
    let running = core_process::refresh_status(state)?.state == CoreProcessState::Running;
    let identity: Option<ConfigRuntimeIdentityView> = if running {
        queries::runtime_identity(Some(ipc_options(state)?))
            .await
            .ok()
            .map(Into::into)
    } else {
        None
    };
    let confirmed = last_transaction.as_ref().is_some_and(|receipt| {
        receipt.profile_id == active.id
            && receipt.effective_digest == digest
            && identity.as_ref().is_some_and(|current| {
                current.core_instance_id == receipt.runtime_identity.core_instance_id
                    && current.config_revision == receipt.runtime_identity.config_revision
            })
    });
    let reason = if confirmed {
        "最终生效配置与最近一次已确认事务一致"
    } else if !running {
        "内核未运行，最终配置尚未提交到运行态"
    } else if identity.is_none() {
        "内核正在运行，但当前无法读取运行态配置身份"
    } else if last_transaction.is_none() {
        "本次客户端会话中尚无已确认的配置事务"
    } else {
        "来源、客户端覆盖或内核实例已在最近一次事务后变化"
    };
    let local_edits = common::lock(state.app_config(), "app_config")?
        .profile_edits
        .get(&active.id)
        .cloned()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| AppError::internal(error.to_string()))?
        .unwrap_or_else(|| json!({}));

    state.configuration().record(candidate.report.clone());
    Ok(ConfigWorkspaceSnapshot {
        profile_id: active.id,
        profile_name: active.name,
        source_updated_at_unix_ms: active.updated_at_unix_ms,
        source_config: source,
        local_edits,
        effective_config: candidate.config,
        composition: Some(candidate.report),
        runtime: ConfigWorkspaceRuntimeView {
            running,
            identity,
            effective_digest: digest,
            confirmed,
            reason: reason.to_string(),
        },
        last_transaction,
    })
}

async fn plan(
    state: &AppState,
    active: &ProxyConfigProfile,
    source_config: Value,
    validate_runtime: bool,
) -> AppResult<ConfigWorkspacePlan> {
    if !source_config.is_object() {
        return Err(AppError::invalid_argument("配置必须是 JSON 对象"));
    }
    let previous_source = active
        .content
        .as_ref()
        .ok_or_else(|| AppError::invalid_argument("当前活动配置没有可编辑的 JSON 内容"))?;
    let previous =
        rule_overlay::compose_effective_candidate_for(state, previous_source, Some(&active.id))?;
    let next =
        rule_overlay::compose_effective_candidate_for(state, &source_config, Some(&active.id))?;
    if validate_runtime {
        ensure_running(state)?;
        let response = ZeroAdapter::new()
            .validate_config(next.config.clone(), ipc_options(state)?)
            .await?;
        ensure_validation_accepted(&response)?;
    }
    let impact = classify_impact(
        previous_source,
        &source_config,
        &previous.config,
        &next.config,
    );
    Ok(ConfigWorkspacePlan {
        impact,
        effective_digest: digest(&next.config)?,
        effective_config: next.config,
    })
}

async fn apply_locked(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: ConfigWorkspaceApplyInput,
) -> AppResult<ConfigTransactionReceipt> {
    ensure_running(state.inner())?;
    let active = active_profile(state.inner())?;
    if active.id != input.profile_id || active.updated_at_unix_ms != input.source_updated_at_unix_ms
    {
        return Err(AppError::conflict(
            "proxy_config",
            input.profile_id,
            "活动配置已切换或被更新；草稿未提交，请刷新后重新预检",
        ));
    }
    let transaction_id = state.configuration().next_transaction_id();
    let started_at_unix_ms = common::now_unix_ms();
    let plan = plan(state.inner(), &active, input.source_config.clone(), true).await?;
    if input.strategy == ConfigApplyStrategy::HotReload && !plan.impact.requires_restart.is_empty()
    {
        return Err(AppError::conflict(
            "config_transaction",
            transaction_id.to_string(),
            "预检发现需要重启的变更；请选择重启应用",
        ));
    }
    let previous_profiles = common::lock(state.proxy_configs(), "proxy_config")?.clone();
    let previous_source = active
        .content
        .as_ref()
        .ok_or_else(|| AppError::invalid_argument("当前活动配置没有可编辑的 JSON 内容"))?;
    let previous_effective = rule_overlay::compose_effective_config_for(
        state.inner(),
        previous_source,
        Some(&active.id),
    )?;

    let applied_identity = match input.strategy {
        ConfigApplyStrategy::HotReload => {
            let identity =
                apply_effective_with_identity(state.inner(), plan.effective_config.clone()).await?;
            if let Err(mut error) = persist_export_retarget(state.clone(), input.source_config) {
                rollback_hot(
                    state.clone(),
                    &previous_profiles,
                    previous_effective.clone(),
                    &mut error,
                )
                .await;
                return Err(error);
            }
            Some(identity)
        }
        ConfigApplyStrategy::Restart => {
            if let Err(mut error) =
                persist_and_restart(app_handle.clone(), state.clone(), input.source_config).await
            {
                rollback_restart(app_handle, state.clone(), &previous_profiles, &mut error).await;
                return Err(error);
            }
            None
        }
    };

    let identity: ConfigRuntimeIdentityView = match applied_identity {
        Some(identity) => identity.into(),
        None => match queries::runtime_identity(Some(ipc_options(state.inner())?)).await {
            Ok(identity) => identity.into(),
            Err(mut error) => {
                error.code = "config_apply_uncertain";
                error.message = format!("配置已提交但最终运行态身份无法确认：{}", error.message);
                match input.strategy {
                    ConfigApplyStrategy::HotReload => {
                        rollback_hot(
                            state.clone(),
                            &previous_profiles,
                            previous_effective,
                            &mut error,
                        )
                        .await;
                    }
                    ConfigApplyStrategy::Restart => {
                        rollback_restart(app_handle, state.clone(), &previous_profiles, &mut error)
                            .await;
                    }
                }
                return Err(error);
            }
        },
    };
    let receipt = ConfigTransactionReceipt {
        transaction_id,
        profile_id: active.id,
        strategy: input.strategy,
        state: "confirmed",
        effective_digest: plan.effective_digest,
        started_at_unix_ms,
        completed_at_unix_ms: common::now_unix_ms(),
        runtime_identity: identity,
    };
    state.configuration().record_transaction(receipt.clone());
    Ok(receipt)
}

fn persist_export_retarget(state: State<'_, AppState>, source: Value) -> AppResult<()> {
    proxy_config::update_active_content(state.inner(), source)?;
    core_config::export_active(state.clone())?;
    proxy_config::retarget_managed_system_proxy(state.inner())
}

async fn persist_and_restart(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    source: Value,
) -> AppResult<()> {
    proxy_config::update_active_content(state.inner(), source)?;
    core_config::export_active(state.clone())?;
    crate::commands::core_process::restart_managed(app_handle)
        .await?
        .require_restored()?;
    proxy_config::retarget_managed_system_proxy(state.inner())
}

async fn rollback_hot(
    state: State<'_, AppState>,
    previous_profiles: &[ProxyConfigProfile],
    previous_effective: Value,
    error: &mut AppError,
) {
    append_recovery(
        error,
        "profile storage",
        proxy_config::restore_profiles(state.inner(), previous_profiles),
    );
    append_recovery_async(
        error,
        "runtime",
        apply_effective(state.inner(), previous_effective).await,
    );
    append_recovery(
        error,
        "export",
        core_config::export_active(state.clone()).map(|_| ()),
    );
    append_recovery(
        error,
        "system proxy",
        proxy_config::retarget_managed_system_proxy(state.inner()),
    );
}

async fn rollback_restart(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    previous_profiles: &[ProxyConfigProfile],
    error: &mut AppError,
) {
    append_recovery(
        error,
        "profile storage",
        proxy_config::restore_profiles(state.inner(), previous_profiles),
    );
    append_recovery(
        error,
        "export",
        core_config::export_active(state.clone()).map(|_| ()),
    );
    let restarted = crate::commands::core_process::restart_managed(app_handle)
        .await
        .and_then(|response| response.require_restored())
        .map(|_| ());
    append_recovery_async(error, "runtime restart", restarted);
    append_recovery(
        error,
        "system proxy",
        proxy_config::retarget_managed_system_proxy(state.inner()),
    );
}

fn append_recovery(error: &mut AppError, component: &str, recovery: AppResult<()>) {
    append_recovery_async(error, component, recovery);
}

fn append_recovery_async(error: &mut AppError, component: &str, recovery: AppResult<()>) {
    if let Err(rollback) = recovery {
        error.code = "config_apply_uncertain";
        error
            .message
            .push_str(&format!("；{component} 恢复失败：{}", rollback.message));
        error.details = Some(json!({
            "cause": error.details,
            "recovery": {"component": component, "code": rollback.code, "message": rollback.message}
        }));
    }
}

fn active_profile(state: &AppState) -> AppResult<ProxyConfigProfile> {
    common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .cloned()
        .ok_or_else(|| AppError::invalid_argument("当前没有活动配置"))
}

fn ensure_running(state: &AppState) -> AppResult<()> {
    if core_process::refresh_status(state)?.state != CoreProcessState::Running {
        return Err(AppError::conflict(
            "core",
            "runtime",
            "内核未运行，无法校验并确认最终生效配置",
        ));
    }
    Ok(())
}

fn ipc_options(state: &AppState) -> AppResult<CoreIpcOptions> {
    let app = common::lock(state.app_config(), "app_config")?;
    Ok(core_config::ipc_options_from_app_config(&app.core))
}

async fn apply_effective(state: &AppState, config: Value) -> AppResult<()> {
    crate::services::config_apply::apply(config, ipc_options(state)?).await
}

async fn apply_effective_with_identity(
    state: &AppState,
    config: Value,
) -> AppResult<queries::KernelRuntimeIdentity> {
    crate::services::config_apply::apply_with_identity(config, ipc_options(state)?).await
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
            return Err(AppError::invalid_argument(format!(
                "内核配置校验失败：{}",
                current
                    .get("errors")
                    .or_else(|| current.get("error"))
                    .unwrap_or(current)
            )));
        }
        if let Some(result) = current.get("result") {
            current = result;
        } else {
            break;
        }
    }
    Err(AppError::internal("无法识别内核配置校验响应"))
}

fn digest(value: &Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| AppError::internal(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
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
        && !requires_restart
            .iter()
            .any(|item| item.section == "runtime.tun")
    {
        requires_restart.push(impact_item(
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
        "outbounds" | "outbound_groups" | "route" | "dns" | "log" | "event_sinks" | "extensions"
    )
}

fn classify_runtime(
    before: Option<&Value>,
    after: Option<&Value>,
    hot_reload: &mut Vec<GuiConfigImpactItem>,
    requires_restart: &mut Vec<GuiConfigImpactItem>,
) {
    let restart_keys = ["tun", "api", "ipc"];
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
    fn classifies_listener_and_tun_changes_as_restart_boundaries() {
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
            .requires_restart
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
}
