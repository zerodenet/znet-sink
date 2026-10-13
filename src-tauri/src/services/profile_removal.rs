//! Fixed-preview profile deletion; source ownership and SQLite relationships
//! stay intact until authoritative runtime coordination has completed.
use crate::{
    errors::{AppError, AppResult},
    kernel::{
        adapter::KernelAdapter,
        zero::{queries, ZeroAdapter},
    },
    models::{
        app_config::AppConfig, core::CoreIpcOptions, core_process::CoreProcessState,
        profile_removal::*, proxy_config::ProxyConfigProfile, subscription::SubscriptionProfile,
    },
    services::{common::lock, core_config, core_process, system_proxy_guard},
    state::app_state::AppState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
mod journal;
#[cfg(test)]
mod tests;

pub(crate) fn require_host(label: &str) -> AppResult<()> {
    if label != "main" {
        return Err(AppError {
            code: "forbidden",
            message: "配置删除仅允许宿主主窗口调用".into(),
            details: None,
        });
    }
    Ok(())
}

struct Local {
    profiles: Vec<ProxyConfigProfile>,
    subscriptions: Vec<SubscriptionProfile>,
    app: AppConfig,
    scope: Value,
}
fn local(state: &AppState) -> AppResult<Local> {
    Ok(Local {
        profiles: lock(state.proxy_configs(), "proxy_config")?.clone(),
        subscriptions: lock(state.subscriptions(), "subscription")?.clone(),
        app: lock(state.app_config(), "app_config")?.clone(),
        scope: serde_json::to_value(state.client_core_snapshot().scope)
            .map_err(|_| AppError::internal("client scope encoding failed"))?,
    })
}
fn hash(value: &Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| AppError::internal("删除预检编码失败"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn guard(local: &Local, runtime: &RemovalRuntime, id: &str) -> AppResult<String> {
    hash(
        &json!({"target":id,"profiles":local.profiles,"subscriptions":local.subscriptions,"app":local.app,"scope":local.scope,"runtime":runtime}),
    )
}
fn changed(id: &str) -> AppError {
    AppError::conflict(
        "profile_removal",
        id,
        "配置、订阅关系、设置或内核实例已变化，请重新预览；未删除原配置",
    )
}
fn offline_error(error: &AppError) -> bool {
    // A timeout says nothing about whether the service still owns forwarding.
    matches!(error.code, "core_unavailable")
}
async fn runtime(state: &AppState, options: CoreIpcOptions) -> AppResult<RemovalRuntime> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Ok(RemovalRuntime {
            state: "platform_unknown".into(),
            core_instance_id: None,
            config_revision: None,
            managed_pid: None,
        });
    }
    let process = core_process::refresh_status(state)?;
    match queries::runtime_identity(Some(options.clone())).await {
        Ok(identity) => Ok(RemovalRuntime {
            state: if process.state == CoreProcessState::Running
                && process.pid.is_some()
                && process.endpoint_path
                    == crate::kernel::protocol::endpoint_from_options(Some(&options))?.path
            {
                "running"
            } else {
                "external"
            }
            .into(),
            core_instance_id: Some(identity.core_instance_id),
            config_revision: Some(identity.config_revision.to_string()),
            managed_pid: process.pid,
        }),
        Err(error)
            if offline_error(&error)
                && process.pid.is_none()
                && !matches!(
                    process.state,
                    CoreProcessState::Starting | CoreProcessState::Running
                ) =>
        {
            Ok(RemovalRuntime {
                state: "offline".into(),
                core_instance_id: None,
                config_revision: None,
                managed_pid: None,
            })
        }
        Err(_) => Ok(RemovalRuntime {
            state: "unknown".into(),
            core_instance_id: None,
            config_revision: None,
            managed_pid: process.pid,
        }),
    }
}
fn options(app: &AppConfig) -> CoreIpcOptions {
    core_config::ipc_options_from_app_config(&app.core)
}
fn target<'a>(local: &'a Local, id: &str) -> AppResult<&'a ProxyConfigProfile> {
    local
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::not_found("proxy_config", id))
}
fn owner_reason(local: &Local, removed: &ProxyConfigProfile) -> Option<String> {
    if removed.managed_source.is_some()
        || local.subscriptions.iter().any(|s| {
            s.target_proxy_config_id.as_deref() == Some(&removed.id) && s.managed_source.is_some()
        })
    {
        Some("插件托管配置或受保护来源关系只能通过所属插件管理；普通删除不会解绑其资源".into())
    } else {
        None
    }
}
fn active_reason(removed: &ProxyConfigProfile, runtime: &RemovalRuntime) -> Option<String> {
    if !removed.active {
        return None;
    }
    match runtime.state.as_str() {
        "running" | "offline" => None,
        "platform_unknown" => Some(
            "平台 VPN/运行服务状态尚未确认；不能删除使用中的配置。请在真实服务适配完成后重新核对"
                .into(),
        ),
        "external" => Some("客户端未持有当前运行实例；请先在其所属宿主停止，再重新检查".into()),
        _ => Some("当前运行及接管状态未知，不能安全删除使用中的配置；请先诊断并重新核对".into()),
    }
}
fn replacement_reason(p: &ProxyConfigProfile) -> Option<String> {
    if p.managed_source.is_some() {
        return Some("托管配置须由来源管理，不作为普通删除的替代材料".into());
    }
    if !p.content.as_ref().is_some_and(Value::is_object) {
        return Some("替代配置没有有效的已解析正文，请先修复".into());
    }
    None
}
pub async fn preview(state: &AppState, id: String) -> AppResult<RemovalPreview> {
    let snapshot = local(state)?;
    let runtime = runtime(state, options(&snapshot.app)).await?;
    let current = local(state)?;
    if hash(
        &json!({"profiles":snapshot.profiles,"subscriptions":snapshot.subscriptions,"app":snapshot.app,"scope":snapshot.scope}),
    )? != hash(
        &json!({"profiles":current.profiles,"subscriptions":current.subscriptions,"app":current.app,"scope":current.scope}),
    )? {
        return Err(changed(&id));
    }
    let removed = target(&current, &id)?;
    let pending = journal::pending(&id)?;
    let blocked_reason = owner_reason(&current, removed)
        .or_else(|| active_reason(removed, &runtime))
        .or_else(|| pending.then(|| "此前删除协调结果未确认；请核对原操作，不能重复提交".into()));
    Ok(RemovalPreview {
        target_id: id.clone(),
        target_name: removed.name.clone(),
        target_version: removed.updated_at_unix_ms.to_string(),
        active: removed.active,
        source_type: if removed.managed_source.is_some() {
            "managed"
        } else if current
            .subscriptions
            .iter()
            .any(|s| s.target_proxy_config_id.as_deref() == Some(&id))
        {
            "subscription"
        } else {
            "local"
        }
        .into(),
        guard: guard(&current, &runtime, &id)?,
        subscriptions: current
            .subscriptions
            .iter()
            .filter(|s| s.target_proxy_config_id.as_deref() == Some(&id))
            .map(|s| RemovalRelation {
                id: s.id.clone(),
                name: s.name.clone(),
            })
            .collect(),
        replacements: current
            .profiles
            .iter()
            .filter(|p| p.id != id)
            .map(|p| RemovalReplacement {
                id: p.id.clone(),
                name: p.name.clone(),
                reason: replacement_reason(p),
            })
            .collect(),
        runtime,
        blocked_reason,
        pending,
        override_count: current.app.profile_edits.get(&id).map_or(0, |e| e.len()),
    })
}
fn candidate(
    local: &Local,
    input: &RemovalInput,
) -> AppResult<(Vec<ProxyConfigProfile>, Vec<SubscriptionProfile>)> {
    let removed = target(local, &input.target_id)?;
    if let Some(reason) = owner_reason(local, removed) {
        return Err(AppError::invalid_argument(reason));
    }
    let replacement = match (&input.action, removed.active) {
        (RemovalAction::Inactive {}, false) => None,
        (RemovalAction::Replace { replacement_id }, true) => {
            let replacement = target(local, replacement_id)?;
            if replacement.id == removed.id {
                return Err(AppError::invalid_argument("不能用正在删除的配置作为替代"));
            }
            if let Some(reason) = replacement_reason(replacement) {
                return Err(AppError::invalid_argument(reason));
            }
            Some(replacement.id.as_str())
        }
        (RemovalAction::Stop {}, true) => None,
        _ => {
            return Err(AppError::invalid_argument(
                "删除动作与当前配置状态不符，请重新预览",
            ))
        }
    };
    let profiles = local
        .profiles
        .iter()
        .filter(|p| p.id != removed.id)
        .cloned()
        .map(|mut p| {
            if removed.active {
                p.active = replacement == Some(p.id.as_str());
            }
            p
        })
        .collect();
    let mut subscriptions = local.subscriptions.clone();
    for s in &mut subscriptions {
        if s.target_proxy_config_id.as_deref() == Some(&removed.id) {
            s.target_proxy_config_id = None;
        }
    }
    Ok((profiles, subscriptions))
}
fn same_local(before: &Local, current: &Local) -> AppResult<bool> {
    Ok(hash(
        &json!({"profiles":before.profiles,"subscriptions":before.subscriptions,"app":before.app}),
    )? == hash(
        &json!({"profiles":current.profiles,"subscriptions":current.subscriptions,"app":current.app}),
    )?)
}
fn uncertain(error: &AppError) -> bool {
    error.is_unavailable()
        || matches!(
            error.code,
            "config_apply_uncertain" | "internal" | "invalid_response" | "conflict"
        )
        || error.details.as_ref().is_some_and(|d| {
            d.get("localRecovery").is_some()
                || d.get("tunTransitionUncertain").is_some()
                || d.pointer("/captureRollback/succeeded") == Some(&json!(false))
        })
}
fn safe_runtime_error(error: AppError, stage: &str) -> AppError {
    // Core/config validation text can contain values from subscription material.
    // This ordinary dialog reports the phase/code; source editing owns the
    // permission-bound field validation view, never a raw error Toast.
    let reason = match error.code {
        "permission_denied" => "内核权限不足",
        "unsupported" | "feature_disabled" => "内核未支持所需能力或协议",
        "conflict" => "实例、版本或捕获状态已变化",
        "invalid_argument" => "配置或引用校验失败",
        _ => "操作被拒绝，请在配置工作区核对兼容性与字段",
    };
    AppError {
        code: error.code,
        message: format!(
            "替代配置{}：{reason}；原本地材料保留",
            if stage == "replacement_validation" {
                "预检失败"
            } else {
                "提交失败"
            }
        ),
        details: Some(json!({"stage":stage,"code":error.code})),
    }
}

fn result(status: &str, message: &str) -> RemovalResult {
    RemovalResult {
        status: status.into(),
        message: message.into(),
        removed_id: None,
        replacement_id: None,
        runtime_stopped: false,
    }
}

pub async fn commit(app: AppHandle, input: RemovalInput) -> AppResult<RemovalResult> {
    let state = app.state::<AppState>();
    let before = local(state.inner())?;
    let ipc = options(&before.app);
    let observed = runtime(state.inner(), ipc.clone()).await?;
    if input.guard != guard(&before, &observed, &input.target_id)?
        || !same_local(&before, &local(state.inner())?)?
    {
        return Err(changed(&input.target_id));
    }
    if journal::pending(&input.target_id)? {
        return Err(AppError::conflict(
            "profile_removal",
            &input.target_id,
            "原删除结果未确认，请先核对；不会自动重放",
        ));
    }
    let removed = target(&before, &input.target_id)?;
    if let Some(reason) =
        owner_reason(&before, removed).or_else(|| active_reason(removed, &observed))
    {
        return Err(AppError::invalid_argument(reason));
    }
    let (next_profiles, next_subscriptions) = candidate(&before, &input)?;
    let mut receipt = None;
    let mut stopped = false;
    if let RemovalAction::Replace { replacement_id } = &input.action {
        let replacement = target(&before, replacement_id)?;
        let source = replacement.content.as_ref().unwrap();
        let effective = crate::services::rule_overlay::compose_effective_config_for(
            state.inner(),
            source,
            Some(replacement_id),
        )?;
        // A deletion must not publish and subsequently discover a failed OS
        // proxy retarget. Different proxy plans require explicit disconnect.
        if system_proxy_guard::is_enabled_by_guard()? {
            let old = crate::configuration::preferences::proxy_settings(state.inner())?;
            let (resolved, source) = crate::configuration::local_edits::resolve(
                &before.app,
                Some(replacement_id),
                source,
            )?;
            let (host, port) = crate::configuration::preferences::endpoint_for(&resolved, &source)?;
            let bypass = if !resolved.overrides.bypass && source.pointer("/route/bypass").is_some()
            {
                vec![]
            } else {
                resolved.local_proxy.bypass.clone()
            };
            if old != (host, port, bypass) {
                return Err(AppError::invalid_argument(
                    "替代配置会改变已接管的桌面代理入口；请先断开系统代理，再重新预览删除",
                ));
            }
        }
        if observed.state == "running" {
            let validation = ZeroAdapter::new()
                .validate_config(effective.clone(), ipc.clone())
                .await
                .map_err(|e| safe_runtime_error(e, "replacement_validation"))?;
            if validation["accepted"] != true || validation["result"]["valid"] != true {
                return Err(AppError::invalid_argument(
                    "替代配置没有通过内核校验；原配置及关系保留",
                ));
            }
            if guard(
                &local(state.inner())?,
                &runtime(state.inner(), ipc.clone()).await?,
                &input.target_id,
            )? != input.guard
            {
                return Err(changed(&input.target_id));
            }
            journal::begin(&input, &observed)?;
            match crate::configuration::capture_apply::apply_profile(
                state.inner(),
                source,
                Some(replacement_id),
                effective,
                ipc.clone(),
            )
            .await
            {
                Ok(applied) => receipt = Some(applied),
                Err(error) if uncertain(&error) => return Ok(result(
                    "unconfirmed",
                    "替代配置的运行提交结果未确认；原配置及订阅关系保留。请核对原操作，勿重复提交",
                )),
                Err(error) => {
                    journal::clear(&input.target_id)?;
                    return Err(safe_runtime_error(error, "runtime_replacement"));
                }
            }
        }
    } else if matches!(input.action, RemovalAction::Stop {}) {
        journal::begin(&input, &observed)?;
        let owned = app.clone();
        let stop = tauri::async_runtime::spawn_blocking(move || {
            let state = owned.state::<AppState>();
            core_process::stop(owned.clone(), state)
        })
        .await
        .map_err(|_| AppError::internal("运行服务停止结果未确认"))?;
        if stop.is_err()
            || runtime(state.inner(), ipc.clone()).await?.state != "offline"
            || system_proxy_guard::is_enabled_by_guard()?
        {
            return Ok(result(
                "unconfirmed",
                "停止或平台接管释放尚未确认；原配置及关系保留，请重新核对",
            ));
        }
        stopped = true;
    }
    if let Some(applied) = &receipt {
        if queries::runtime_identity(Some(ipc.clone()))
            .await
            .ok()
            .as_ref()
            != Some(&applied.identity)
        {
            return Ok(result(
                "unconfirmed",
                "替代应用后实例或版本已变化；原配置保留，请核对，未执行删除或盲目回滚",
            ));
        }
    } else if removed.active && runtime(state.inner(), ipc.clone()).await?.state != "offline" {
        return Ok(result(
            "unconfirmed",
            "运行停止状态发生变化；原配置及关系保留，请重新核对",
        ));
    }
    if !same_local(&before, &local(state.inner())?)? {
        return Ok(result(
            "unconfirmed",
            "运行协调后本地关系已变化，本次删除尚未发布；请核对运行配置及最新来源，不会盲目回滚或重放",
        ));
    }
    if !journal::pending(&input.target_id)? {
        journal::begin(&input, &observed)?;
    }
    if let Err(error) = crate::configuration::persistence::commit_relational_if_current(
        state.inner(),
        &before.profiles,
        &before.subscriptions,
        &before.app,
        next_profiles,
        next_subscriptions,
    ) {
        // Restore only an exact confirmed receipt; its coordinator rejects a
        // changed instance/revision. The local relationship graph is unchanged.
        if let Some(applied) = receipt {
            if applied.restore(state.inner()).await.is_err() {
                return Ok(result(
                    "unconfirmed",
                    "本地删除及替代运行恢复未确认，请核对当前配置、持久关系与运行态",
                ));
            }
        }
        if uncertain(&error) {
            return Ok(result(
                "unconfirmed",
                "本地删除持久化或恢复结果未确认，请核对配置及关系；不会重复提交",
            ));
        }
        journal::clear(&input.target_id)?;
        return Err(error);
    }
    // A stale journal after successful local publication is harmless: recovery
    // verifies that the exact target is absent before acknowledging completion.
    let runtime_applied = receipt.is_some();
    let _ = app.emit(
        "subscriptions:updated",
        json!({"profileId":input.target_id,"source":"profile_removal"}),
    );
    let journal_clean = journal::clear(&input.target_id).is_ok();
    Ok(RemovalResult {
        status: "confirmed".into(),
        message: if stopped {
            "已确认停止运行并删除配置；关联订阅保留且解除原关联"
        } else if removed.active && !runtime_applied {
            "已删除本地配置并选定替代；内核离线，替代配置已保存待应用。关联订阅保留且解除原关联"
        } else {
            "配置已删除；关联订阅保留且解除原关联"
        }
        .to_string()
            + if journal_clean {
                ""
            } else {
                "；恢复记录清理待重试"
            },
        removed_id: Some(input.target_id),
        replacement_id: match input.action {
            RemovalAction::Replace { replacement_id } => Some(replacement_id),
            _ => None,
        },
        runtime_stopped: stopped,
    })
}

pub async fn reconcile(state: &AppState, id: String) -> AppResult<RemovalResult> {
    let had_pending = journal::pending(&id)?;
    let current = local(state)?;
    let durable = crate::services::domain_store::load_all_from_dir(&crate::services::data_dir()?)?;
    if serde_json::to_value((&current.profiles, &current.subscriptions))
        .map_err(|_| AppError::internal("本地关系编码失败"))?
        != serde_json::to_value((&durable.proxy_configs, &durable.subscriptions))
            .map_err(|_| AppError::internal("持久关系编码失败"))?
    {
        return Ok(result(
            "unconfirmed",
            "内存与持久配置关系不一致；请诊断本地存储，原删除仍待核对",
        ));
    }
    if !current.profiles.iter().any(|p| p.id == id) {
        if had_pending {
            journal::clear(&id)?;
        }
        let mut confirmed = result(
            "confirmed",
            "已核对目标配置不在本地库；关联状态以最新配置列表为准",
        );
        confirmed.removed_id = Some(id);
        return Ok(confirmed);
    }
    if !had_pending {
        return Ok(result(
            "not_deleted",
            "已核对当前配置仍在本地库；请重新预览当前范围",
        ));
    }
    let observed = runtime(state, options(&current.app)).await?;
    if journal::unchanged_inactive(&id, &guard(&current, &observed, &id)?)? {
        journal::clear(&id)?;
        return Ok(result(
            "not_deleted",
            "已核对本地配置及关联原样保留；旧删除意图已结束，可重新预览。没有操作 VPN",
        ));
    }
    if observed.state == "offline" && !system_proxy_guard::is_enabled_by_guard()? {
        journal::clear(&id)?;
        return Ok(result(
            "not_deleted",
            "已核对运行停止，原配置仍保留；旧删除意图已结束。可重新预览，核对不会自动删除或启动",
        ));
    }
    Ok(result("unconfirmed", "原配置仍在本地库，运行协调结果尚未确认；请先诊断并确认服务停止，再核对。不会重放替代应用或删除"))
}
