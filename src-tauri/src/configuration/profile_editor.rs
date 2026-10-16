//! Ordinary manual profile text editing. This owner never calls IPC, retarget,
//! TUN or a runtime transaction. Native validation returns the original Value.
use super::{
    native_validation, persistence,
    workspace::{self, Binding},
};
use crate::{
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig,
        profile_editor::{DraftInput, SaveInput, Saved, Snapshot, Validated},
        proxy_config::ProxyConfigProfile,
        subscription::SubscriptionProfile,
    },
    services::{
        app_database::workspace_recovery,
        common::{lock, now_unix_ms},
        file_history::{self, Domain},
        proxy_config,
    },
    state::app_state::AppState,
};
use serde::Serialize;

// This is a source-material check, not a compiled-feature/platform/runtime ack.
const VALIDATION_SCOPE: &str = "Zero 原生来源 schema 与引用校验；未授权的规则文件和 GeoIP 依赖不读取。客户端覆盖、实际编译协议、平台和运行生效仍需启用时预检。";
const MAX_BYTES: usize = 16 * 1024 * 1024;

fn conflict() -> AppError {
    AppError {
        code: "profile_editor_conflict",
        message: "配置来源、关系、设置、规则或实例已变化；草稿保留，请明确刷新后再保存".into(),
        details: None,
    }
}
fn readonly() -> AppError {
    AppError {
        code: "profile_editor_read_only",
        message: "此来源不能通过普通配置编辑器修改；请到原来源管理，草稿保留".into(),
        details: None,
    }
}
fn valid_id(id: Option<&str>) -> AppResult<()> {
    if id.is_some_and(|id| id.is_empty() || id.len() > 512 || id.chars().any(char::is_control)) {
        return Err(AppError::invalid_argument("配置身份不合法"));
    }
    Ok(())
}
/// The boolean controls source access, independently of write eligibility.
fn admission(
    profile: &ProxyConfigProfile,
    subscriptions: &[SubscriptionProfile],
) -> (bool, Option<&'static str>) {
    if profile.managed_source.is_some()
        || subscriptions.iter().any(|s| {
            s.target_proxy_config_id.as_deref() == Some(&profile.id) && s.managed_source.is_some()
        })
    {
        (
            false,
            Some("此配置由插件托管；普通编辑器不读取或修改正文，请到来源插件管理"),
        )
    } else if profile.content.is_none() {
        (
            false,
            Some("来源没有普通可读正文；受保护材料不能降级为普通配置"),
        )
    } else if profile.kernel != "zero" {
        (false, Some("此普通编辑器只支持 Zero 原生配置"))
    } else if subscriptions
        .iter()
        .any(|s| s.target_proxy_config_id.as_deref() == Some(&profile.id))
        || profile
            .path
            .as_deref()
            .is_some_and(|p| p.starts_with("https://") || p.starts_with("http://"))
    {
        (
            true,
            Some("此正文来自订阅，普通编辑器只读；请同步原订阅或通过受支持的客户端覆盖管理"),
        )
    } else {
        (true, None)
    }
}

#[derive(Serialize)]
struct EditorBinding<'a> {
    target: Option<&'a str>,
    workspace: &'a Binding,
}
struct Context {
    id: Option<String>,
    profiles: Vec<ProxyConfigProfile>,
    subscriptions: Vec<SubscriptionProfile>,
    app: AppConfig,
    target: Option<ProxyConfigProfile>,
    binding: Binding,
}
impl Context {
    fn token(&self) -> AppResult<String> {
        workspace::digest(&EditorBinding {
            target: self.id.as_deref(),
            workspace: &self.binding,
        })
    }
}

/// Lock order matches publication: profiles -> subscriptions -> rules -> app,
/// then AppHistory -> RulesHistory. Admission precedes cloning target material.
fn context(state: &AppState, id: Option<&str>) -> AppResult<Context> {
    valid_id(id)?;
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subscriptions = lock(state.subscriptions(), "subscription")?;
    let target = match id {
        Some(id) => Some(
            profiles
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| AppError::not_found("proxy_config", id))?,
        ),
        None => None,
    };
    if let Some(target) = target {
        if admission(target, &subscriptions).1.is_some() {
            return Err(readonly());
        }
        workspace::ensure_unblocked(state, &target.id)?;
    }
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    let core = state.client_core_snapshot();
    if core.scope_generation.0 == 0 || core.scope_generation.0 > 9_007_199_254_740_991 {
        return Err(conflict());
    }
    let dir = workspace::recovery::directory(state)?;
    let app_file = file_history::acquire(&dir.join("app-config.json"), Domain::AppConfig)?;
    let rules_file = file_history::acquire(&dir.join("rule-sets.json"), Domain::RuleSets)?;
    let binding = Binding {
        host_session_id: state.host_session_id()?.to_owned(),
        domain_epoch: workspace_recovery::current_epoch(&dir)?,
        app_history: app_file.token()?,
        rules_history: rules_file.token()?,
        scope: core.scope,
        generation: core.scope_generation.0,
        source_digest: workspace::digest(&target)?,
        graph_digest: workspace::digest(&(&*profiles, &*subscriptions))?,
        app_digest: workspace::digest(&*app)?,
        rules_digest: workspace::digest(&*rules)?,
    };
    // Verify actual stored typed files as well as their history identity.
    if workspace::digest(&crate::services::app_config_store::read_current_locked(
        &app_file,
    )?)? != binding.app_digest
        || workspace::digest(&crate::services::domain_store::read_rule_sets_locked(
            &rules_file,
        )?)? != binding.rules_digest
    {
        return Err(conflict());
    }
    Ok(Context {
        id: id.map(str::to_owned),
        target: target.cloned(),
        profiles: profiles.clone(),
        subscriptions: subscriptions.clone(),
        app: app.clone(),
        binding,
    })
}

pub(crate) fn snapshot(state: &AppState, id: Option<&str>) -> AppResult<Snapshot> {
    valid_id(id)?;
    if let Some(id) = id {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let subscriptions = lock(state.subscriptions(), "subscription")?;
        let target = profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::not_found("proxy_config", id))?;
        let (may_read, reason) = admission(target, &subscriptions);
        if let Some(reason) = reason {
            // Never serialize protected/managed bodies, even to a temporary DTO.
            return Ok(Snapshot {
                target_id: Some(target.id.clone()),
                create_target_id: None,
                binding: None,
                name: target.name.clone(),
                source_json: if may_read {
                    target.content.as_ref().map(workspace::pretty).transpose()?
                } else {
                    None
                },
                read_only_reason: Some(reason.into()),
                source_updated_at_unix_ms: Some(target.updated_at_unix_ms.to_string()),
                validation_scope: VALIDATION_SCOPE.into(),
            });
        }
    }
    snapshot_from(&context(state, id)?)
}
fn snapshot_from(context: &Context) -> AppResult<Snapshot> {
    let binding = context.token()?;
    Ok(Snapshot {
        target_id: context.id.clone(),
        create_target_id: context.id.is_none().then(|| format!("manual-{binding}")),
        binding: Some(binding),
        name: context
            .target
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        source_json: context
            .target
            .as_ref()
            .and_then(|p| p.content.as_ref())
            .map(workspace::pretty)
            .transpose()?,
        read_only_reason: None,
        source_updated_at_unix_ms: context
            .target
            .as_ref()
            .map(|p| p.updated_at_unix_ms.to_string()),
        validation_scope: VALIDATION_SCOPE.into(),
    })
}
fn checked(state: &AppState, id: Option<&str>, expected: &str) -> AppResult<Context> {
    let context = context(state, id)?;
    if context.token()? != expected {
        return Err(conflict());
    }
    Ok(context)
}
pub(crate) fn validate(state: &AppState, input: DraftInput) -> AppResult<Validated> {
    let context = checked(state, input.id.as_deref(), &input.binding)?;
    let value = native_validation::validate_text(&input.source_json)?;
    checked(state, input.id.as_deref(), &context.token()?)?;
    Ok(Validated {
        state: "validated",
        source_json: workspace::pretty(&value)?,
        validation_scope: VALIDATION_SCOPE.into(),
    })
}

/// Caller holds proxy_config_operation. No runtime status refresh or command.
pub(crate) fn save(state: &AppState, input: SaveInput) -> AppResult<Saved> {
    let context = checked(state, input.id.as_deref(), &input.binding)?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
        return Err(AppError::invalid_argument(
            "配置名称应为1至512字节且不含控制字符",
        ));
    }
    let source = native_validation::validate_text(&input.source_json)?;
    let mut target = match &context.target {
        Some(target) => target.clone(),
        None => {
            // Freeze creation identity before publication so a lost delivery
            // can be checked without a new save or a new target identity.
            let id = format!("manual-{}", context.token()?);
            if context.profiles.iter().any(|p| p.id == id) {
                return Err(conflict());
            }
            ProxyConfigProfile {
                id,
                name: String::new(),
                kernel: "zero".into(),
                format: "json".into(),
                path: None,
                content: None,
                active: false,
                managed_source: None,
                updated_at_unix_ms: 0,
                capabilities: Default::default(),
            }
        }
    };
    target.name = name.into();
    target.content = Some(source);
    target.updated_at_unix_ms = now_unix_ms().max(
        target
            .updated_at_unix_ms
            .checked_add(1)
            .ok_or_else(conflict)?,
    );
    target.capabilities = proxy_config::analyze_capabilities(target.content.as_ref());
    let mut next = context.profiles.clone();
    if let Some(existing) = next.iter_mut().find(|p| p.id == target.id) {
        *existing = target.clone();
    } else {
        next.push(target.clone());
    }
    persistence::commit_workspace_if_current(
        state,
        &context.profiles,
        &context.subscriptions,
        &context.app,
        next,
        &context.binding,
    )?;
    // Delivery can fail after durable publication; never compensate or replay.
    let after = snapshot(state, Some(&target.id)).map_err(|_| saved_unknown())?;
    let canonical = workspace::pretty(target.content.as_ref().ok_or_else(saved_unknown)?)?;
    if after.name != target.name
        || after.source_json.as_deref() != Some(canonical.as_str())
        || after.source_updated_at_unix_ms.as_deref()
            != Some(target.updated_at_unix_ms.to_string().as_str())
        || after.read_only_reason.is_some()
    {
        return Err(saved_unknown());
    }
    Ok(Saved {
        state: "saved_pending_apply",
        snapshot: after,
    })
}
fn saved_unknown() -> AppError {
    AppError {
        code: "profile_editor_save_unknown",
        message:
            "本地保存已进入发布阶段，但最终回执未确认；请刷新核对原配置，不会自动重试或恢复旧材料"
                .into(),
        details: None,
    }
}

/// Desktop-only host-selected file read. It deliberately does not parse JSON,
/// resolve siblings, load rule artifacts or grant directory authority.
pub(crate) fn read_file(
    manager: &znet_client_core::capability::Manager,
    path: String,
    native: bool,
) -> AppResult<String> {
    use std::{
        collections::BTreeSet,
        sync::{atomic::AtomicBool, Arc},
        time::Duration,
    };
    use znet_client_capabilities::files;
    use znet_client_core::capability::Budget;
    if native {
        return Err(AppError {
            code: "profile_editor_native_path_unavailable",
            message: "移动端请通过系统文档选择读取内容；不会读取桌面路径".into(),
            details: None,
        });
    }
    let selection = files::Selection::from_user_path(path.into());
    let permission = selection.permission();
    let grants = BTreeSet::from([permission]);
    let failure = |_| AppError {
        code: "profile_editor_file_read_failed",
        message: "所选配置文件读取失败、超限或授权已失效；草稿保留".into(),
        details: None,
    };
    let policy = manager
        .admit(
            "builtin.profile-editor-file".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .map_err(failure)?;
    policy
        .authorize(grants, Duration::from_secs(30))
        .map_err(failure)?;
    let lease = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: MAX_BYTES,
                timeout: Duration::from_secs(30),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(failure)?;
    let bytes = files::read(&lease, &selection, MAX_BYTES)
        .and_then(|r| r.take(&lease))
        .map_err(failure)?;
    String::from_utf8(bytes)
        .map_err(|_| AppError::invalid_argument("配置文件必须使用UTF-8编码；草稿保留"))
}

#[cfg(test)]
mod tests;
