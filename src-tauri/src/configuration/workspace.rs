//! Client-owned workspace bindings. This module never reads IPC or changes TUN.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig, config_workspace::*, proxy_config::ProxyConfigProfile,
        subscription::SubscriptionProfile,
    },
    services::{
        common::{lock, now_unix_ms},
        proxy_config, rule_overlay,
    },
    state::app_state::AppState,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Mutex};
const MAX_BYTES: usize = 16 * 1024 * 1024;
const TTL: u64 = 300_000;
#[derive(Clone, Serialize)]
pub(crate) struct Binding {
    pub host_session_id: String,
    pub domain_epoch: i64,
    pub scope: ClientScope,
    pub generation: u64,
    pub source_digest: String,
    pub graph_digest: String,
    pub app_digest: String,
    pub rules_digest: String,
}
pub(crate) struct Context {
    pub profile: ProxyConfigProfile,
    pub profiles: Vec<ProxyConfigProfile>,
    pub subscriptions: Vec<SubscriptionProfile>,
    pub app: AppConfig,
    pub binding: Binding,
}
#[derive(Clone)]
pub(crate) struct Prepared {
    pub binding: Binding,
    pub source: Value,
    pub effective: Value,
    pub capture_digest: String,
    pub frozen_capture_parameters: Value,
    pub identity: Option<crate::kernel::zero::queries::KernelRuntimeIdentity>,
    pub plan: ConfigWorkspacePlan,
    pub operation: ConfigWorkspaceOperation,
    pub confirmed_binding: Option<String>,
}
#[derive(Default)]
pub(crate) struct Plans(
    Mutex<BTreeMap<String, Prepared>>,
    Mutex<Option<std::path::PathBuf>>,
    Mutex<BTreeMap<String, recovery::Review>>,
);
pub(crate) fn conflict() -> AppError {
    AppError::conflict(
        "config_workspace",
        "",
        "来源、覆盖、规则集或实例已变化；草稿保留，请明确刷新并重新预检",
    )
}
pub(crate) fn digest(value: &impl Serialize) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| AppError::internal("工作区身份编码失败"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
pub(crate) fn pretty(value: &Value) -> AppResult<String> {
    serde_json::to_string_pretty(value).map_err(|_| AppError::internal("工作区文本编码失败"))
}
pub(crate) fn read_only(profile: &ProxyConfigProfile) -> Option<&'static str> {
    if profile.managed_source.is_some() {
        Some("此配置由插件来源托管；普通工作区不读取或改写正文，请到来源插件管理")
    } else if profile.content.is_none() {
        Some("此来源没有可供普通工作区读取的正文；受保护或不支持的材料不能降级为普通配置")
    } else if profile.kernel != "zero" {
        Some("当前工作区只支持 Zero 原生配置")
    } else {
        None
    }
}
pub(crate) fn context(state: &AppState) -> AppResult<Context> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let profile = profiles
        .iter()
        .find(|p| p.active)
        .ok_or_else(|| AppError::invalid_argument("当前没有活动配置"))?;
    // Admission before cloning/accessing source content or constructing views.
    if let Some(reason) = read_only(profile) {
        return Err(AppError {
            code: "workspace_source_read_only",
            message: reason.into(),
            details: None,
        });
    }
    let profile = profile.clone();
    let profiles = profiles.clone();
    let subscriptions = lock(state.subscriptions(), "subscription")?.clone();
    let app = lock(state.app_config(), "app_config")?.clone();
    let snapshot = state.client_core_snapshot();
    if snapshot.scope_generation.0 == 0 || snapshot.scope_generation.0 > 9_007_199_254_740_991 {
        return Err(conflict());
    }
    let binding = Binding {
        host_session_id: state.host_session_id()?.to_owned(),
        domain_epoch: crate::services::app_database::workspace_recovery::current_epoch(
            &recovery::directory(state)?,
        )?,
        scope: snapshot.scope,
        generation: snapshot.scope_generation.0,
        source_digest: digest(&profile)?,
        graph_digest: digest(&(&profiles, &subscriptions))?,
        app_digest: digest(&app)?,
        rules_digest: digest(&*lock(state.rule_sets(), "rule_set")?)?,
    };
    Ok(Context {
        profile,
        profiles,
        subscriptions,
        app,
        binding,
    })
}
pub(crate) fn verify(state: &AppState, expected: &Binding) -> AppResult<Context> {
    state.ensure_client_scope_generation(&expected.scope, expected.generation)?;
    let current = context(state)?;
    if digest(&current.binding)? != digest(expected)? {
        return Err(conflict());
    }
    Ok(current)
}
pub(crate) fn from_input(
    state: &AppState,
    input: &ConfigWorkspaceDraftInput,
) -> AppResult<(Context, Value, Value)> {
    let context = context(state)?;
    if digest(&context.binding)? != input.binding {
        return Err(conflict());
    }
    ensure_unblocked(state, &context.profile.id)?;
    let source = parse(&input.source_json)?;
    let candidate = rule_overlay::compose_effective_candidate_with_app(
        state,
        &source,
        Some(&context.profile.id),
        &context.app,
    )
    .map_err(|_| {
        AppError::invalid_argument("客户端组合配置校验失败；请检查覆盖与规则依赖，草稿保留")
    })?;
    validate(&candidate.config)?;
    verify(state, &context.binding)?;
    Ok((context, source, candidate.config))
}
pub(crate) fn parse(text: &str) -> AppResult<Value> {
    if text.len() > MAX_BYTES {
        return Err(AppError::invalid_argument("配置超过16MiB工作区预算"));
    }
    // The actual native parser checks schema, endpoint projection and all references.
    zero_config::RuntimeConfig::parse(text).map_err(native_error)?;
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| AppError::invalid_argument("配置 JSON 无效"))
}
pub(crate) fn validate(value: &Value) -> AppResult<()> {
    zero_config::RuntimeConfig::parse(&value.to_string())
        .map(|_| ())
        .map_err(native_error)
}
fn native_error(error: zero_config::ConfigError) -> AppError {
    use zero_config::ConfigError::*;
    let (path, line, column) = match error {
        ParseConfig(error) => ("", Some(error.line()), Some(error.column())),
        InvalidDns(_) => ("runtime.dns", None, None),
        InvalidRuntime(_) => ("runtime", None, None),
        InvalidInbound(_) | DuplicateInboundListen { .. } => ("inbounds", None, None),
        InvalidOutbound(_) => ("outbounds", None, None),
        InvalidOutboundGroup(_) => ("outbound_groups", None, None),
        InvalidRuleSet(_) | UndefinedRuleSetTag { .. } => ("route.rule_sets", None, None),
        InvalidRouteAction(_) | InvalidRuleCondition(_) | UndefinedRouteTargetTag { .. } => {
            ("route", None, None)
        }
        InvalidApi(_) => ("api", None, None),
        InvalidMode(_) => ("mode", None, None),
        UnsupportedSchemaVersion { .. } => ("schema_version", None, None),
        _ => ("", None, None),
    };
    AppError {
        code: "workspace_validation_failed",
        message: "Zero 原生配置校验未通过；请检查字段、类型及引用，草稿保留".into(),
        details: Some(
            json!({"errors":[{"field_path":path,"message":"原生 schema、类型或引用不合法","line":line,"column":column}]}),
        ),
    }
}
pub(crate) fn local_snapshot(state: &AppState) -> AppResult<ConfigWorkspaceSnapshot> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let profile = profiles
        .iter()
        .find(|p| p.active)
        .ok_or_else(|| AppError::invalid_argument("当前没有活动配置"))?;
    if let Some(reason) = read_only(profile) {
        return Ok(ConfigWorkspaceSnapshot {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            binding: None,
            read_only_reason: Some(reason.into()),
            source_updated_at_unix_ms: profile.updated_at_unix_ms.to_string(),
            source_json: None,
            local_edits_json: None,
            effective_json: None,
            composition: None,
            pending_apply: false,
            pending_operation: None,
            runtime: ConfigWorkspaceRuntimeView {
                running: None,
                identity: None,
                effective_digest: String::new(),
                confirmed: false,
                reason: "来源只读；普通工作区未读取正文或运行配置".into(),
            },
            last_transaction: None,
        });
    }
    drop(profiles);
    let context = context(state)?;
    let source = context.profile.content.as_ref().ok_or_else(conflict)?;
    let candidate = rule_overlay::compose_effective_candidate_with_app(
        state,
        source,
        Some(&context.profile.id),
        &context.app,
    )?;
    let mut app = context.app.clone();
    super::local_edits::migrate_legacy(&mut app, Some(&context.profile.id));
    let mut edits = app.client_edits.unwrap_or_default();
    if let Some(endpoints) = app
        .profile_edits
        .get(&context.profile.id)
        .and_then(|v| v.get("endpoints"))
    {
        edits.insert("endpoints".into(), endpoints.clone());
    }
    verify(state, &context.binding)?;
    let last_transaction = recovery::completed_transaction(state, &context, &candidate.config)?
        .or_else(|| state.configuration().transaction());
    Ok(ConfigWorkspaceSnapshot { profile_id: context.profile.id.clone(), profile_name: context.profile.name.clone(), binding: Some(digest(&context.binding)?), read_only_reason: None,
        source_updated_at_unix_ms: context.profile.updated_at_unix_ms.to_string(), source_json: Some(pretty(source)?), local_edits_json: Some(pretty(&json!(edits))?), effective_json: Some(pretty(&candidate.config)?),
        pending_apply: true, pending_operation: pending_operation(state, &context.profile.id)?, composition: Some(candidate.report), runtime: ConfigWorkspaceRuntimeView { running: None, identity: None, effective_digest: digest(&candidate.config)?, confirmed: false, reason: "本地配置已保存，运行状态未核对；待应用或重新核对。原生 schema 校验不代表运行协议及平台兼容".into() }, last_transaction })
}
pub(crate) fn save(
    state: &AppState,
    input: ConfigWorkspaceDraftInput,
) -> AppResult<ConfigWorkspaceSaveResult> {
    save_at(state, input, None)
}
pub(crate) fn save_at(
    state: &AppState,
    input: ConfigWorkspaceDraftInput,
    directory: Option<&std::path::Path>,
) -> AppResult<ConfigWorkspaceSaveResult> {
    let (context, source, _) = from_input(state, &input)?;
    publish_at(state, &context, source, directory)?;
    // Pure local view: no process status refresh, IPC query, retarget or TUN.
    Ok(ConfigWorkspaceSaveResult {
        state: "saved_pending_apply",
        snapshot: local_snapshot(state).map_err(|_| AppError {
            code: "workspace_local_save_uncertain",
            message:
                "本地保存已执行，但返回视图未确认；原草稿保留，请读取来源核对，不会自动重复保存"
                    .into(),
            details: None,
        })?,
    })
}
fn publish_at(
    state: &AppState,
    context: &Context,
    source: Value,
    directory: Option<&std::path::Path>,
) -> AppResult<()> {
    verify(state, &context.binding)?;
    let mut profiles = context.profiles.clone();
    let target = profiles
        .iter_mut()
        .find(|p| p.id == context.profile.id)
        .ok_or_else(conflict)?;
    if read_only(target).is_some() {
        return Err(conflict());
    }
    target.content = Some(source);
    target.updated_at_unix_ms = now_unix_ms().max(target.updated_at_unix_ms.saturating_add(1));
    target.capabilities = proxy_config::analyze_capabilities(target.content.as_ref());
    if let Some(directory) = directory {
        return super::persistence::commit_checked(
            state,
            &context.profiles,
            &context.subscriptions,
            &context.app,
            profiles,
            context.subscriptions.clone(),
            true,
            Some(directory),
            Some(&context.binding.rules_digest),
            Some((&context.binding.scope, context.binding.generation)),
        );
    }
    super::persistence::commit_workspace_if_current(
        state,
        &context.profiles,
        &context.subscriptions,
        &context.app,
        profiles,
        &context.binding,
    )
}
pub(crate) fn store(state: &AppState, mut prepared: Prepared) -> AppResult<ConfigWorkspacePlan> {
    verify(state, &prepared.binding)?;
    let now = now_unix_ms();
    let mut plans = lock(&state.configuration().plans.0, "workspace_plans")?;
    plans.retain(|_, p| {
        matches!(
            p.operation.state,
            "submitted_unknown" | "runtime_confirmed_pending_local"
        ) || p.plan.expires_at_unix_ms.parse::<u64>().unwrap_or(0) > now
    });
    while plans.len() >= 4 {
        let completed = plans
            .iter()
            .filter(|(_, p)| p.operation.state == "confirmed")
            .min_by_key(|(_, p)| p.plan.expires_at_unix_ms.clone())
            .map(|(id, _)| id.clone());
        if let Some(id) = completed {
            plans.remove(&id);
        } else {
            break;
        }
    }
    if plans.len() >= 4 {
        return Err(AppError::conflict(
            "config_workspace",
            "capacity",
            "已有四个工作区事务；请核对原事务，不能自动重放",
        ));
    }
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| AppError::internal("无法创建工作区事务"))?;
    let id = random
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    prepared.plan.plan_id = id.clone();
    prepared.plan.expires_at_unix_ms = now.saturating_add(TTL).to_string();
    prepared.operation.plan_id = id.clone();
    let plan = prepared.plan.clone();
    plans.insert(id, prepared);
    Ok(plan)
}
fn peer_pending(plans: &BTreeMap<String, Prepared>, id: &str, binding: &Binding) -> bool {
    plans.iter().any(|(other_id, p)| {
        other_id != id
            && p.binding.scope.profile_id == binding.scope.profile_id
            && matches!(
                p.operation.state,
                "submitted_unknown" | "runtime_confirmed_pending_local"
            )
    })
}
fn pending_error() -> AppError {
    AppError {
        code: "config_apply_uncertain",
        message: "同一来源的另一原事务尚未确认；此前预检也不能绕过该屏障再次应用".into(),
        details: None,
    }
}
pub(crate) fn review(state: &AppState, id: &str) -> AppResult<Prepared> {
    // Current source admission precedes reading the stored candidate body.
    let current = context(state)?;
    if recovery::pending(state, &current.profile.id)?.is_some() {
        return Err(pending_error());
    }
    let prepared = {
        let plans = lock(&state.configuration().plans.0, "workspace_plans")?;
        let plan = plans.get(id).ok_or_else(|| {
            AppError::invalid_argument("原预检不存在，请保留草稿并重新核对；不会重放")
        })?;
        if peer_pending(&plans, id, &plan.binding) {
            return Err(pending_error());
        }
        plan.clone()
    };
    if prepared.operation.state != "prepared" {
        return Err(AppError {
            code: "config_apply_uncertain",
            message: "原事务已提交或已结束；只能核对原操作，不能重复应用".into(),
            details: None,
        });
    }
    if prepared.plan.expires_at_unix_ms.parse::<u64>().unwrap_or(0) <= now_unix_ms() {
        return Err(conflict());
    }
    verify(state, &prepared.binding)?;
    if let Some(reason) = &prepared.plan.apply_unavailable_reason {
        return Err(AppError::invalid_argument(reason.clone()));
    }
    Ok(prepared)
}
pub(crate) fn submit(state: &AppState, id: &str) -> AppResult<Prepared> {
    review(state, id)?;
    let mut plans = lock(&state.configuration().plans.0, "workspace_plans")?;
    let binding = plans.get(id).ok_or_else(conflict)?.binding.clone();
    if peer_pending(&plans, id, &binding) {
        return Err(pending_error());
    }
    let plan = plans.get_mut(id).ok_or_else(conflict)?;
    if plan.operation.state != "prepared" {
        return Err(conflict());
    }
    let context = verify(state, &plan.binding)?;
    recovery::prepare_submitted(state, plan, &context)?;
    plan.operation.state = "submitted_unknown";
    plan.operation.reason = Some("已进入运行变更；未取得完整回执前不能重放".into());
    Ok(plan.clone())
}
pub(crate) fn reject_before_submit(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.configuration().plans.0, "workspace_plans")?;
    if let Some(plan) = plans.get_mut(id) {
        if plan.operation.state == "prepared" {
            plan.operation.state = "rejected";
            plan.operation.reason = Some(
                "副作用前的来源或运行身份预检未通过；未发送运行变更，草稿保留，请重新预检".into(),
            );
        }
    }
    Ok(())
}
/// Only the owning capture entry's typed, pre-side-effect gate may call this.
/// This is not an API for clearing an uncertain operation after runtime I/O.
pub(crate) fn reject_without_runtime_mutation(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.configuration().plans.0, "workspace_plans")?;
    let plan = plans.get_mut(id).ok_or_else(conflict)?;
    if plan.operation.state != "submitted_unknown" || plan.operation.receipt.is_some() {
        return Err(conflict());
    }
    recovery::rejected(state, id)?;
    plan.operation.state = "rejected";
    plan.operation.reason =
        Some("冻结接管参数在副作用前已变化；未发送运行变更，草稿保留，请重新预检".into());
    Ok(())
}
pub(crate) fn runtime_confirmed(
    state: &AppState,
    id: &str,
    receipt: ConfigTransactionReceipt,
) -> AppResult<()> {
    let mut plans = lock(&state.configuration().plans.0, "workspace_plans")?;
    let plan = plans.get_mut(id).ok_or_else(conflict)?;
    recovery::confirmed(state, id, &receipt)?;
    plan.operation.state = "runtime_confirmed_pending_local";
    plan.operation.receipt = Some(receipt);
    plan.operation.reason = Some(
        "原运行材料及接管计划已确认；本地来源尚未确认发布。不会重放、覆盖新来源或声称全部成功"
            .into(),
    );
    Ok(())
}
pub(crate) fn pending_operation(
    state: &AppState,
    profile_id: &str,
) -> AppResult<Option<ConfigWorkspaceOperation>> {
    if let Some(meta) = recovery::pending(state, profile_id)? {
        return Ok(Some(recovery::view(&meta)));
    }
    Ok(lock(&state.configuration().plans.0, "workspace_plans")?
        .values()
        .find(|p| {
            p.binding
                .scope
                .profile_id
                .as_ref()
                .is_some_and(|id| id.0 == profile_id)
                && matches!(
                    p.operation.state,
                    "submitted_unknown" | "runtime_confirmed_pending_local"
                )
        })
        .map(|p| p.operation.clone()))
}
pub(crate) fn operation(state: &AppState, id: &str) -> AppResult<ConfigWorkspaceOperation> {
    if let Some(meta) = recovery::metadata(state, id)? {
        let current = context(state)?;
        if current.profile.id != meta.profile_id {
            return Err(conflict());
        }
        return Ok(recovery::view(&meta));
    }
    let (mut operation, confirmed_binding) = {
        let plans = lock(&state.configuration().plans.0, "workspace_plans")?;
        let prepared = plans.get(id).ok_or_else(|| {
            AppError::invalid_argument("原事务记录不可用；不能用相同内容的其他回执确认或自动重放")
        })?;
        (
            prepared.operation.clone(),
            prepared.confirmed_binding.clone(),
        )
    };
    if operation.state == "confirmed"
        && confirmed_binding.as_deref() != Some(digest(&context(state)?.binding)?.as_str())
    {
        operation.state = "submitted_unknown";
        operation.reason = Some("原事务后来源、覆盖或实例已变化；历史确认不能当作当前状态".into());
    }
    Ok(operation)
}

pub(crate) fn ensure_unblocked(state: &AppState, profile_id: &str) -> AppResult<()> {
    if recovery::pending(state, profile_id)?.is_some() {
        return Err(pending_error());
    }
    if lock(&state.configuration().plans.0, "workspace_plans")?
        .values()
        .any(|p| {
            p.binding
                .scope
                .profile_id
                .as_ref()
                .is_some_and(|id| id.0 == profile_id)
                && matches!(
                    p.operation.state,
                    "submitted_unknown" | "runtime_confirmed_pending_local"
                )
        })
    {
        return Err(AppError {
            code: "config_apply_uncertain",
            message: "此来源仍有原事务结果未确认；先核对原操作，不会以新计划重放".into(),
            details: None,
        });
    }
    Ok(())
}
pub(crate) fn discard(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.configuration().plans.0, "workspace_plans")?;
    if plans
        .get(id)
        .is_some_and(|p| matches!(p.operation.state, "prepared" | "rejected"))
    {
        plans.remove(id);
    }
    Ok(())
}

pub(crate) mod recovery;

#[cfg(test)]
pub(crate) mod tests;
