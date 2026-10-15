//! Restore one client-owned endpoint preference through ordinary composition,
//! acknowledged runtime configuration and endpoint reconciliation. Plans and
//! uncertain submissions survive UI and process loss; recovery never replays.
use crate::{
    configuration::endpoints,
    errors::{AppError, AppResult},
    kernel::{
        adapter::KernelAdapter,
        zero::{queries, ZeroAdapter},
    },
    models::{app_config::AppConfig, core::CoreIpcOptions, endpoint_restoration::*},
    services::{common::lock, network_endpoint, rule_overlay},
    state::app_state::AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{future::Future, path::PathBuf};
mod history;
mod journal;
use crate::services::file_history::WriteReceipt;
pub use journal::EndpointRestoration;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    #[serde(default)]
    files: Option<history::Files>,
    target: RestorationTarget,
    source_digest: String,
    source_updated_at: u64,
    receipt_digest: String,
    snapshot: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    #[serde(default)]
    owner: Option<history::Owner>,
    #[serde(default)]
    local_write_started: bool,
    #[serde(default)]
    save_only: bool,
    #[serde(default)]
    app_write: Option<WriteReceipt>,
    plan: RestorationPlan,
    binding: Binding,
    app_digest: String,
    overlay: Value,
    clear_started: bool,
    receipt: RestorationReceipt,
}
fn journal_path() -> AppResult<PathBuf> {
    Ok(crate::services::data_dir()?.join("endpoint-restoration-v1.json"))
}
fn digest(value: &impl Serialize) -> AppResult<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|e| AppError::internal(e.to_string()))?)
    ))
}
fn conflict(message: &str) -> AppError {
    AppError::conflict("endpoint_restoration", "", message)
}
fn active(
    state: &AppState,
) -> AppResult<(
    crate::models::proxy_config::ProxyConfigProfile,
    Value,
    AppConfig,
)> {
    let profile = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .cloned()
        .ok_or_else(|| conflict("当前没有活动配置"))?;
    if profile.managed_source.is_some() {
        return Err(conflict(
            "托管配置的来源恢复由所属插件管理，不能通过普通配置路径恢复",
        ));
    }
    let source = profile
        .content
        .clone()
        .ok_or_else(|| conflict("受保护或缺少普通来源配置，不能恢复客户端覆盖"))?;
    let app = lock(state.app_config(), "app_config")?.clone();
    Ok((profile, source, app))
}
fn public_snapshot(row: &Value) -> Value {
    let mut snapshot = json!({});
    for key in [
        "endpoint_id",
        "tag",
        "protocol",
        "core_instance_id",
        "config_revision",
        "intent_revision",
        "generation",
        "enabled",
        "allowed",
        "effective",
        "state",
        "state_source",
        "inbound_tags",
        "outbound_tags",
    ] {
        snapshot[key] = row[key].clone();
    }
    snapshot["configuration"] = json!({"origin":row.pointer("/configuration/origin"),"enabled":row.pointer("/configuration/enabled"),"directions":row.pointer("/configuration/directions")});
    snapshot["supported"] = json!({"directions":row.pointer("/supported/directions"),"operations":row.pointer("/supported/operations")});
    snapshot
}
fn same_resource(a: &Value, b: &Value) -> bool {
    [
        "endpoint_id",
        "tag",
        "protocol",
        "inbound_tags",
        "outbound_tags",
    ]
    .iter()
    .all(|key| a[*key] == b[*key])
        && a.pointer("/configuration/origin") == Some(&json!("canonical"))
        && b.pointer("/configuration/origin") == Some(&json!("canonical"))
        && a.pointer("/supported/directions") == b.pointer("/supported/directions")
}
fn exact_snapshot(a: &Value, b: &Value) -> bool {
    same_resource(a, b)
        && [
            "core_instance_id",
            "config_revision",
            "intent_revision",
            "generation",
        ]
        .iter()
        .all(|key| a[*key] == b[*key])
}
fn overlay<'a>(app: &'a AppConfig, profile: &str, tag: &str) -> Option<&'a Value> {
    app.profile_edits
        .get(profile)
        .and_then(|edits| edits.get("endpoints"))
        .and_then(|prefs| prefs.get(tag))
}
fn pending(journal: &journal::Journal, profile: &str, id: &str) -> Option<Record> {
    journal
        .records
        .values()
        .find(|record| {
            record.receipt.phase == RestorePhase::SavedPending
                && record.binding.target.profile_id == profile
                && record.binding.target.endpoint_id == id
        })
        .cloned()
}
/// Root calls this only while holding the same configuration operation lock as
/// managed::catalog. Cached bindings never authorize a different source body.
pub(crate) async fn remember_catalog(
    state: &AppState,
    profile_id: Option<&str>,
    rows: &[Value],
    options: CoreIpcOptions,
) -> AppResult<Vec<RestorationTarget>> {
    let Some(profile_id) = profile_id else {
        return Ok(vec![]);
    };
    let (profile, source, app) = active(state)?;
    if profile.id != profile_id {
        return Ok(vec![]);
    }
    let Some(receipt) = state.configuration().transaction() else {
        return Err(conflict("尚无来源配置应用回执，请先核对配置工作区"));
    };
    if receipt.profile_id != profile.id || receipt.state != "confirmed" {
        return Err(conflict("来源配置回执属于其他配置或尚未确认"));
    }
    let identity = queries::runtime_identity(Some(options)).await?;
    if receipt.runtime_identity.core_instance_id != identity.core_instance_id
        || receipt.runtime_identity.config_revision != identity.config_revision
    {
        return Err(conflict(
            "来源配置回执的实例或 revision 已变化，请重新核对工作区",
        ));
    }
    let current = rule_overlay::compose_effective_candidate_with_app(
        state,
        &source,
        Some(&profile.id),
        &app,
    )?;
    let current_digest = digest(&current.config)?;
    let files = history::capture(state, &app)?;
    let mut journal = state.endpoint_restoration().read()?;
    let mut targets = vec![];
    for row in rows {
        if row["core_instance_id"] != identity.core_instance_id
            || row["config_revision"].as_u64() != Some(identity.config_revision)
            || row.pointer("/configuration/origin").and_then(Value::as_str) != Some("canonical")
            || !endpoints::owns(&source, row)
        {
            continue;
        }
        let target_app = match endpoints::restore_candidate(&app, &profile.id, &source, row) {
            Ok(next) => next,
            Err(_) => {
                if overlay(&app, &profile.id, row["tag"].as_str().unwrap_or("")).is_some()
                    || pending(
                        &journal,
                        &profile.id,
                        row["endpoint_id"].as_str().unwrap_or(""),
                    )
                    .filter(|record| history::published_matches(record, &files))
                    .is_none()
                {
                    continue;
                }
                app.clone()
            }
        };
        let candidate = rule_overlay::compose_effective_candidate_with_app(
            state,
            &source,
            Some(&profile.id),
            &target_app,
        )?;
        // An offline local removal leaves the acknowledged running digest
        // at the previous version. Rebind only when reconstructing the exact
        // recorded pre-save AppConfig proves this specific pending removal.
        let trusted_pending = if let Some(previous) = pending(
            &journal,
            &profile.id,
            row["endpoint_id"].as_str().unwrap_or(""),
        ) {
            history::published_matches(&previous, &files)
                && pending_receipt_matches(
                    &previous,
                    &app,
                    &profile.id,
                    &source,
                    profile.updated_at_unix_ms,
                    &receipt.effective_digest,
                )?
        } else {
            false
        };
        if receipt.effective_digest != current_digest
            && receipt.effective_digest != digest(&candidate.config)?
            && !trusted_pending
        {
            continue;
        }
        let snapshot = public_snapshot(row);
        let binding_id = digest(&json!([
            profile.id,
            digest(&source)?,
            snapshot,
            receipt.effective_digest,
            files
        ]))?;
        let target = RestorationTarget {
            binding_id: binding_id.clone(),
            endpoint_id: row["endpoint_id"].as_str().unwrap().into(),
            profile_id: profile.id.clone(),
            tag: row["tag"].as_str().unwrap().into(),
            protocol: row["protocol"].as_str().unwrap().into(),
        };
        journal.bindings.insert(
            binding_id,
            Binding {
                files: Some(files.clone()),
                target: target.clone(),
                source_digest: digest(&source)?,
                source_updated_at: profile.updated_at_unix_ms,
                receipt_digest: receipt.effective_digest.clone(),
                snapshot,
            },
        );
        targets.push(target);
    }
    // Keep current bindings plus those referenced by durable operations.
    let keep = targets
        .iter()
        .map(|target| target.binding_id.clone())
        .chain(
            journal
                .records
                .values()
                .map(|record| record.binding.target.binding_id.clone()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    journal.bindings.retain(|key, _| keep.contains(key));
    state.endpoint_restoration().replace(journal)?;
    Ok(targets)
}
// Material comparison only. Callers additionally require the exact original
// owned file receipt and current history tokens; hashes alone grant nothing.
fn pending_receipt_matches(
    record: &Record,
    app: &AppConfig,
    profile: &str,
    source: &Value,
    source_version: u64,
    receipt_digest: &str,
) -> AppResult<bool> {
    if record.receipt.phase != RestorePhase::SavedPending
        || record.binding.target.profile_id != profile
        || record.binding.source_digest != digest(source)?
        || record.binding.source_updated_at != source_version
        || record.plan.previous_effective_digest != receipt_digest
        || overlay(app, profile, &record.binding.target.tag).is_some()
    {
        return Ok(false);
    }
    let mut previous = app.clone();
    previous
        .profile_edits
        .entry(profile.into())
        .or_default()
        .entry("endpoints".into())
        .or_insert_with(|| json!({}))[&record.binding.target.tag] = record.overlay.clone();
    Ok(digest(&previous)? == record.app_digest)
}
fn check_source(
    binding: &Binding,
    profile: &crate::models::proxy_config::ProxyConfigProfile,
    source: &Value,
) -> AppResult<()> {
    if profile.id != binding.target.profile_id
        || profile.updated_at_unix_ms != binding.source_updated_at
        || digest(source)? != binding.source_digest
        || !endpoints::owns(source, &binding.snapshot)
    {
        return Err(conflict("配置、来源版本或端点声明已变化，请重新预检"));
    }
    Ok(())
}
fn receipt_for(plan: &RestorationPlan) -> RestorationReceipt {
    RestorationReceipt {
        operation_id: plan.plan_id.clone(),
        target: plan.target.clone(),
        phase: RestorePhase::Prepared,
        local_saved: false,
        runtime_confirmed: false,
        apply_identity: None,
        endpoint: None,
        message: "预检已创建，尚未保存或提交".into(),
    }
}
pub(crate) async fn preview(
    state: &AppState,
    input: PreviewInput,
    options: CoreIpcOptions,
) -> AppResult<RestorationPlan> {
    let mut journal = state.endpoint_restoration().read()?;
    let binding = journal
        .bindings
        .get(&input.binding_id)
        .cloned()
        .ok_or_else(|| conflict("没有可信的来源绑定；请先核对配置工作区并刷新端点"))?;
    let (profile, source, app) = active(state)?;
    check_source(&binding, &profile, &source)?;
    history::require_binding(state, &binding, &app)?;
    let owner = history::owner(state)?;
    let files = binding
        .files
        .as_ref()
        .ok_or_else(|| conflict("来源绑定缺少文件历史"))?;
    let client_scope = state.client_core_snapshot().scope;
    if journal.records.values().any(|record| {
        record.binding.target.profile_id == profile.id
            && record.binding.target.endpoint_id == binding.target.endpoint_id
            && record.binding.source_digest == binding.source_digest
            && record.plan.client_scope == client_scope
            && matches!(
                record.receipt.phase,
                RestorePhase::Applying
                    | RestorePhase::Applied
                    | RestorePhase::Clearing
                    | RestorePhase::RuntimeConfirmed
                    | RestorePhase::ResultUnknown
            )
    }) {
        return Err(conflict(
            "此端点有未核对事务，请先读取原恢复结果；不会自动重放",
        ));
    }
    let pending = pending(&journal, &profile.id, &binding.target.endpoint_id)
        .filter(|record| history::published_matches(record, files));
    let next = match endpoints::restore_candidate(&app, &profile.id, &source, &binding.snapshot) {
        Ok(next) => next,
        Err(error) => {
            if let Some(previous) = &pending {
                if previous.binding.source_digest != binding.source_digest
                    || overlay(&app, &profile.id, &binding.target.tag).is_some()
                {
                    return Err(error);
                }
                app.clone()
            } else {
                return Err(error);
            }
        }
    };
    let before = rule_overlay::compose_effective_candidate_with_app(
        state,
        &source,
        Some(&profile.id),
        &app,
    )?;
    let candidate = rule_overlay::compose_effective_candidate_with_app(
        state,
        &source,
        Some(&profile.id),
        &next,
    )?;
    let mut base_confirmed = digest(&before.config)? == binding.receipt_digest
        || digest(&candidate.config)? == binding.receipt_digest;
    if let Some(pending) = &pending {
        let mut original = app.clone();
        original
            .profile_edits
            .entry(profile.id.clone())
            .or_default()
            .entry("endpoints".into())
            .or_insert_with(|| json!({}))[&binding.target.tag] = pending.overlay.clone();
        base_confirmed |= history::published_matches(pending, files)
            && digest(&original)? == pending.app_digest
            && (pending.plan.previous_effective_digest == binding.receipt_digest
                || pending.plan.effective_digest == binding.receipt_digest);
    }
    let runtime_available = base_confirmed
        && match network_endpoint::get(&binding.target.endpoint_id, options).await {
            Ok(current) => {
                if !exact_snapshot(&binding.snapshot, &current) {
                    return Err(conflict(
                        "端点实例、意图或 generation 已变化，请刷新后重新预检",
                    ));
                }
                true
            }
            Err(error) if error.is_unavailable() => false,
            Err(error) => return Err(error),
        };
    let overlay = app
        .profile_edits
        .get(&profile.id)
        .and_then(|edits| edits.get("endpoints"))
        .and_then(|prefs| prefs.get(&binding.target.tag))
        .cloned()
        .or_else(|| pending.as_ref().map(|record| record.overlay.clone()))
        .ok_or_else(|| conflict("端点覆盖已不存在"))?;
    let removed_fields = ["enabled", "directions"]
        .iter()
        .filter(|key| overlay.get(**key).is_some())
        .map(|key| (*key).to_owned())
        .collect();
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| AppError::internal("无法生成恢复操作标识"))?;
    let created = crate::services::common::now_unix_ms();
    state.ensure_client_scope(Some(&client_scope))?;
    let plan = RestorationPlan {
        plan_id: random.iter().map(|byte| format!("{byte:02x}")).collect(),
        target: binding.target.clone(),
        source_digest: binding.source_digest.clone(),
        source_updated_at_unix_ms: binding.source_updated_at.to_string(),
        previous_effective_digest: digest(&before.config)?,
        effective_digest: digest(&candidate.config)?,
        removed_fields,
        runtime_available,
        client_scope,
        created_at_unix_ms: created.to_string(),
        expires_at_unix_ms: created.saturating_add(300_000).to_string(),
        impact: "仅移除此端点的客户端启用/方向覆盖，保留来源、其他端点与 DNS；运行应用会重载配置并协调此端点，可能中断相关连接。".into(),
    };
    history::require_binding(state, &binding, &app)?;
    let record = Record {
        owner: Some(owner),
        local_write_started: false,
        save_only: false,
        app_write: None,
        receipt: receipt_for(&plan),
        plan: plan.clone(),
        binding,
        app_digest: digest(&app)?,
        overlay,
        clear_started: false,
    };
    journal.records.insert(plan.plan_id.clone(), record);
    while journal.records.len() > 32 {
        let removable = journal
            .records
            .iter()
            .find(|(_, record)| {
                matches!(
                    record.receipt.phase,
                    RestorePhase::Confirmed | RestorePhase::Rejected
                )
            })
            .map(|(id, _)| id.clone());
        if let Some(id) = removable {
            journal.records.remove(&id);
        } else {
            break;
        }
    }
    journal.records.retain(|_, record| {
        record.receipt.phase != RestorePhase::Prepared || record.plan.plan_id == plan.plan_id
    });
    state.endpoint_restoration().replace(journal)?;
    Ok(plan)
}
fn save_record(state: &AppState, record: &Record) -> AppResult<()> {
    let mut journal = state.endpoint_restoration().read()?;
    journal
        .records
        .insert(record.plan.plan_id.clone(), record.clone());
    state.endpoint_restoration().replace(journal)
}
fn restored_intent(row: &Value, source: &Value) -> bool {
    if row["enabled"].as_bool().is_none()
        || row["configuration"]["enabled"].as_bool().is_none()
        || ["inbound", "outbound"].iter().any(|direction| {
            row["allowed"][*direction].as_bool().is_none()
                || row["configuration"]["directions"][*direction]
                    .as_bool()
                    .is_none()
        })
    {
        return false;
    }
    if row["state_source"] != "config"
        || row["enabled"] != row["configuration"]["enabled"]
        || row["allowed"] != row["configuration"]["directions"]
    {
        return false;
    }
    source["endpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| {
            item["tag"] == row["tag"] && item.pointer("/protocol/type") == row.get("protocol")
        })
        .is_some_and(|item| {
            item.get("enabled")
                .is_none_or(|value| value == &row["enabled"])
                && item
                    .get("directions")
                    .is_none_or(|value| value == &row["allowed"])
        })
}
trait Backend: Sync {
    fn endpoint(&self, id: &str) -> impl Future<Output = AppResult<Value>> + Send;
    fn validate(&self, config: &Value) -> impl Future<Output = AppResult<()>> + Send;
    fn apply(
        &self,
        record: &Record,
        previous: &AppConfig,
        expected: &Value,
        config: &Value,
    ) -> impl Future<Output = AppResult<RuntimeReceipt>> + Send;
    fn clear(
        &self,
        record: &Record,
        previous: &AppConfig,
        row: &Value,
    ) -> impl Future<Output = AppResult<Value>> + Send;
    fn checkpoint(&self, record: &Record) -> AppResult<()>;
    fn guard_files(&self, record: &Record, expected: &AppConfig) -> AppResult<()>;
    fn publish(
        &self,
        record: &Record,
        expected: &AppConfig,
        next: AppConfig,
    ) -> AppResult<WriteReceipt>;
}
struct Live<'a> {
    state: &'a AppState,
    options: CoreIpcOptions,
}
impl Backend for Live<'_> {
    async fn endpoint(&self, id: &str) -> AppResult<Value> {
        network_endpoint::get(id, self.options.clone()).await
    }
    async fn validate(&self, config: &Value) -> AppResult<()> {
        let response = ZeroAdapter::new()
            .validate_config(config.clone(), self.options.clone())
            .await?;
        let mut current = &response;
        for _ in 0..6 {
            if current["valid"] == true {
                return Ok(());
            }
            if current["valid"] == false || current["ok"] == false {
                return Err(AppError::invalid_argument(
                    "内核拒绝恢复后的配置；本地覆盖尚未移除",
                ));
            }
            if let Some(next) = current.get("result") {
                current = next
            } else {
                break;
            }
        }
        Err(AppError::internal("无法识别配置校验回执，不能应用"))
    }
    async fn apply(
        &self,
        record: &Record,
        previous: &AppConfig,
        expected: &Value,
        config: &Value,
    ) -> AppResult<RuntimeReceipt> {
        let current = queries::runtime_identity(Some(self.options.clone())).await?;
        if expected["core_instance_id"] != current.core_instance_id
            || expected["config_revision"].as_u64() != Some(current.config_revision)
        {
            return Err(conflict("配置提交前内核身份已变化"));
        }
        self.guard_files(record, previous)?;
        let identity = crate::configuration::apply::apply_with_identity(
            self.state.capabilities(),
            config.clone(),
            self.options.clone(),
        )
        .await?;
        if identity.core_instance_id != current.core_instance_id {
            return Err(conflict("配置应用期间内核实例已变化"));
        }
        Ok(RuntimeReceipt {
            core_instance_id: identity.core_instance_id,
            config_revision: identity.config_revision.to_string(),
        })
    }
    async fn clear(&self, record: &Record, previous: &AppConfig, row: &Value) -> AppResult<Value> {
        self.guard_files(record, previous)?;
        network_endpoint::control_at_revision(
            network_endpoint::ControlInput {
                endpoint_id: row["endpoint_id"]
                    .as_str()
                    .ok_or_else(|| conflict("端点身份缺失"))?
                    .into(),
                core_instance_id: row["core_instance_id"]
                    .as_str()
                    .ok_or_else(|| conflict("内核身份缺失"))?
                    .into(),
                expected_intent_revision: row["intent_revision"]
                    .as_u64()
                    .ok_or_else(|| conflict("端点意图缺失"))?,
                action: network_endpoint::Action::ClearOverrides {},
            },
            self.options.clone(),
            row["config_revision"].as_u64(),
        )
        .await
    }
    fn checkpoint(&self, record: &Record) -> AppResult<()> {
        save_record(self.state, record)
    }
    fn guard_files(&self, record: &Record, expected: &AppConfig) -> AppResult<()> {
        history::require_owner(self.state, record)?;
        history::require(self.state, record, expected)
    }
    fn publish(
        &self,
        record: &Record,
        expected: &AppConfig,
        next: AppConfig,
    ) -> AppResult<WriteReceipt> {
        history::require_owner(self.state, record)?;
        history::publish(self.state, record, expected, next)
    }
}
fn phase(
    backend: &impl Backend,
    record: &mut Record,
    next: RestorePhase,
    message: &str,
) -> AppResult<()> {
    record.receipt.phase = next;
    record.receipt.message = message.into();
    backend.checkpoint(record)
}
fn uncertain_result(
    backend: &impl Backend,
    record: &mut Record,
    message: &str,
) -> RestorationReceipt {
    record.receipt.phase = RestorePhase::ResultUnknown;
    record.receipt.message = message.into();
    if backend.checkpoint(record).is_err() {
        record
            .receipt
            .message
            .push_str("；恢复记录保存失败，请核对原操作，不能重放");
    }
    record.receipt.clone()
}
async fn execute(
    backend: &impl Backend,
    record: &mut Record,
    source: &Value,
    previous: &AppConfig,
    next: AppConfig,
    candidate: &Value,
) -> AppResult<RestorationReceipt> {
    let row = backend.endpoint(&record.binding.target.endpoint_id).await?;
    if !exact_snapshot(&record.binding.snapshot, &row) {
        return Err(conflict("确认后端点资源、实例或版本已变化；请重新预检"));
    }
    if row["state"] == "starting" || row["state"] == "stopping" {
        return Err(conflict("端点正在协调，请等待后重新预检"));
    }
    backend.validate(candidate).await?;
    backend.guard_files(record, previous)?;
    // Persist before either network mutation. Repeated calls with this plan
    // only read its receipt, including after process loss.
    phase(
        backend,
        record,
        RestorePhase::Applying,
        "正在应用来源配置；本地覆盖尚未移除",
    )?;
    let applied = match backend.apply(record, previous, &row, candidate).await {
        Ok(identity) => identity,
        Err(_) => {
            return Ok(uncertain_result(
                backend,
                record,
                "配置应用结果未确认；保留客户端覆盖，请核对原操作，不会自动重放",
            ))
        }
    };
    record.receipt.apply_identity = Some(applied.clone());
    if phase(
        backend,
        record,
        RestorePhase::Applied,
        "配置回执已确认，正在核对目标端点",
    )
    .is_err()
    {
        return Ok(uncertain_result(
            backend,
            record,
            "运行配置已确认，但回执记录保存失败；客户端覆盖尚未移除",
        ));
    }
    let current = match backend.endpoint(&record.binding.target.endpoint_id).await {
        Ok(row) => row,
        Err(_) => {
            return Ok(uncertain_result(
                backend,
                record,
                "配置已确认但端点无法读取；客户端覆盖尚未移除",
            ))
        }
    };
    if !same_resource(&record.binding.snapshot, &current)
        || current["core_instance_id"] != applied.core_instance_id
        || current["config_revision"]
            .as_u64()
            .map(|value| value.to_string())
            .as_ref()
            != Some(&applied.config_revision)
    {
        return Ok(uncertain_result(
            backend,
            record,
            "配置应用后目标资源或实例未能核对；没有发送清除运行覆盖命令",
        ));
    }
    if backend.guard_files(record, previous).is_err() {
        return Ok(uncertain_result(
            backend,
            record,
            "配置应用已返回，但本地文件历史已变化；未继续清除或保存",
        ));
    }
    record.clear_started = true;
    phase(
        backend,
        record,
        RestorePhase::Clearing,
        "正在清除目标运行覆盖；新 revision 和意图已重新核对",
    )?;
    let cleared = match backend.clear(record, previous, &current).await {
        Ok(row) => row,
        Err(_) => {
            return Ok(uncertain_result(
                backend,
                record,
                "清除运行覆盖结果未确认；保留客户端覆盖，请核对原操作",
            ))
        }
    };
    if !same_resource(&record.binding.snapshot, &cleared)
        || cleared["core_instance_id"] != applied.core_instance_id
        || cleared["config_revision"] != current["config_revision"]
        || !restored_intent(&cleared, source)
    {
        return Ok(uncertain_result(
            backend,
            record,
            "端点回执尚未确认来源值；客户端覆盖尚未移除",
        ));
    }
    record.receipt.endpoint = Some(public_snapshot(&cleared));
    record.receipt.runtime_confirmed = true;
    if backend.guard_files(record, previous).is_err() {
        return Ok(uncertain_result(
            backend,
            record,
            "运行回执已收到，但本地文件历史已变化；未继续保存旧候选",
        ));
    }
    record.local_write_started = true;
    phase(
        backend,
        record,
        RestorePhase::RuntimeConfirmed,
        "配置应用和端点清除回执已确认，尚待本地保存",
    )?;
    match backend.publish(record, previous, next) {
        Ok(receipt) => record.app_write = Some(receipt),
        Err(_) => {
            return Ok(uncertain_result(
                backend,
                record,
                "运行恢复已确认，但客户端覆盖保存冲突或失败；没有声称回滚，请核对原操作",
            ))
        }
    }
    record.receipt.local_saved = true;
    if phase(
        backend,
        record,
        RestorePhase::Confirmed,
        "此端点来源恢复已由配置、端点回执和本地持久化共同确认",
    )
    .is_err()
    {
        return Ok(uncertain_result(
            backend,
            record,
            "运行与本地恢复已确认，但事务记录保存失败，请重新核对",
        ));
    }
    Ok(record.receipt.clone())
}
fn save_pending(
    backend: &impl Backend,
    record: &mut Record,
    previous: &AppConfig,
    next: AppConfig,
) -> AppResult<RestorationReceipt> {
    backend.guard_files(record, previous)?;
    record.local_write_started = true;
    record.save_only = true;
    phase(
        backend,
        record,
        RestorePhase::SavedPending,
        "准备保存此端点覆盖移除意图；运行态尚未提交",
    )?;
    match backend.publish(record, previous, next) {
        Ok(receipt) => record.app_write = Some(receipt),
        Err(error) if error.code == "conflict" => {
            phase(
                backend,
                record,
                RestorePhase::Rejected,
                "本地保存被拒绝，运行态没有提交",
            )?;
            return Ok(record.receipt.clone());
        }
        Err(_) => {
            return Ok(uncertain_result(
                backend,
                record,
                "本地保存结果未确认；保留原操作，不会重写或重放",
            ))
        }
    }
    record.receipt.local_saved = true;
    if phase(
        backend,
        record,
        RestorePhase::SavedPending,
        "此端点客户端覆盖已保存移除，运行态待应用；没有宣称 VPN 或端点运行已恢复",
    )
    .is_err()
    {
        return Ok(uncertain_result(
            backend,
            record,
            "本地保存已完成，但恢复记录无法保存；请核对本地与运行状态",
        ));
    }
    Ok(record.receipt.clone())
}
fn validate_plan(
    plan: &RestorationPlan,
    scope: &crate::client_core::ClientScope,
    now: u64,
) -> AppResult<()> {
    let created = plan
        .created_at_unix_ms
        .parse::<u64>()
        .map_err(|_| conflict("恢复计划时间无效，请重新预检"))?;
    let expires = plan
        .expires_at_unix_ms
        .parse::<u64>()
        .map_err(|_| conflict("恢复计划时间无效，请重新预检"))?;
    if created > now
        || expires <= now
        || expires <= created
        || expires.saturating_sub(created) > 300_000
    {
        return Err(conflict("恢复预检已过期，请重新预检"));
    }
    if &plan.client_scope != scope {
        return Err(conflict("恢复确认所属配置或内核实例已变化，请重新预检"));
    }
    Ok(())
}
pub(crate) async fn apply(
    state: &AppState,
    input: ApplyInput,
    options: CoreIpcOptions,
) -> AppResult<RestorationReceipt> {
    let journal = state.endpoint_restoration().read()?;
    let mut record = journal
        .records
        .get(&input.plan_id)
        .cloned()
        .ok_or_else(|| conflict("恢复预检不存在，请重新预检"))?;
    if record.receipt.phase != RestorePhase::Prepared {
        return Ok(record.receipt);
    }
    history::require_owner(state, &record)?;
    state.ensure_client_scope(Some(&record.plan.client_scope))?;
    validate_plan(
        &record.plan,
        &state.client_core_snapshot().scope,
        crate::services::common::now_unix_ms(),
    )?;
    let (profile, source, app) = active(state)?;
    check_source(&record.binding, &profile, &source)?;
    if digest(&app)? != record.app_digest {
        return Err(conflict("客户端设置或覆盖已变化，请重新预检"));
    }
    history::require(state, &record, &app)?;
    let next =
        match endpoints::restore_candidate(&app, &profile.id, &source, &record.binding.snapshot) {
            Ok(next) => next,
            Err(error) => {
                if overlay(&app, &profile.id, &record.binding.target.tag).is_none()
                    && pending(&journal, &profile.id, &record.binding.target.endpoint_id).is_some()
                {
                    app.clone()
                } else {
                    return Err(error);
                }
            }
        };
    let candidate = rule_overlay::compose_effective_candidate_with_app(
        state,
        &source,
        Some(&profile.id),
        &next,
    )?;
    if digest(&candidate.config)? != record.plan.effective_digest {
        return Err(conflict("最终合成配置已变化，请重新预检"));
    }
    if input.mode == RestoreMode::SavePending {
        return save_pending(&Live { state, options }, &mut record, &app, next);
    }
    if !record.plan.runtime_available {
        return Err(conflict(
            "预检时运行态不可读取；请仅保存待应用，或恢复内核后重新预检",
        ));
    }
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    if ![
        "network_endpoint_control_v1",
        "network_endpoint_control_preconditions_v1",
    ]
    .iter()
    .all(|feature| caps.features.iter().any(|value| value == feature))
        || !record.binding.snapshot["supported"]["operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|value| value == "clear_overrides"))
    {
        return Err(conflict("当前内核或端点没有安全清除运行覆盖能力"));
    }
    let receipt = execute(
        &Live { state, options },
        &mut record,
        &source,
        &app,
        next,
        &candidate.config,
    )
    .await?;
    record_workspace_receipt(state, &record);
    Ok(receipt)
}
/// Reconciliation is read-only on the kernel. A missing apply receipt can
/// never be inferred from a newer revision or from matching visible fields.
pub(crate) async fn reconcile(
    state: &AppState,
    operation_id: String,
    options: CoreIpcOptions,
) -> AppResult<RestorationReceipt> {
    let journal = state.endpoint_restoration().read()?;
    let mut record = journal
        .records
        .get(&operation_id)
        .cloned()
        .ok_or_else(|| conflict("恢复事务不存在"))?;
    if matches!(
        record.receipt.phase,
        RestorePhase::Prepared | RestorePhase::Confirmed | RestorePhase::Rejected
    ) {
        return Ok(record.receipt);
    }
    // Read-only reconciliation may inspect a completed operation; publishing
    // local preferences from an old client scope is never allowed.
    state.ensure_client_scope(Some(&record.plan.client_scope))?;
    if record.save_only || record.receipt.phase == RestorePhase::SavedPending {
        let (profile, source, _app) = active(state)?;
        check_source(&record.binding, &profile, &source)?;
        record.app_write = Some(history::confirm_local(state, &record, &source)?);
        record.receipt.local_saved = true;
        record.receipt.phase = RestorePhase::SavedPending;
        record.receipt.message =
            "原本地保存已由固定文件历史回执核对，运行态仍待应用；没有重写正文".into();
        save_record(state, &record)?;
        return Ok(record.receipt);
    }
    let (profile, source, app) = active(state)?;
    check_source(&record.binding, &profile, &source)?;
    let Some(applied) = record.receipt.apply_identity.clone() else {
        return Ok(record.receipt);
    };
    // Applied-only records have never dispatched clear_overrides. They remain
    // uncertain; reconciliation must not silently send that missing mutation.
    if !record.clear_started {
        return Ok(record.receipt);
    }
    let identity = queries::runtime_identity(Some(options.clone())).await?;
    let row = network_endpoint::get(&record.binding.target.endpoint_id, options).await?;
    if applied.core_instance_id != identity.core_instance_id
        || applied.config_revision != identity.config_revision.to_string()
        || row["core_instance_id"] != identity.core_instance_id
        || row["config_revision"].as_u64() != Some(identity.config_revision)
        || !same_resource(&record.binding.snapshot, &row)
        || !restored_intent(&row, &source)
    {
        return Ok(record.receipt);
    }
    record.receipt.runtime_confirmed = true;
    record.receipt.endpoint = Some(public_snapshot(&row));
    if record.local_write_started {
        record.app_write = Some(history::confirm_local(state, &record, &source)?);
    } else {
        history::require_owner(state, &record)?;
        history::require(state, &record, &app)?;
        let next =
            endpoints::restore_candidate(&app, &profile.id, &source, &record.binding.snapshot)?;
        record.local_write_started = true;
        save_record(state, &record)?;
        record.app_write = Some(history::publish(state, &record, &app, next)?);
    }
    record.receipt.local_saved = true;
    record.receipt.phase = RestorePhase::Confirmed;
    record.receipt.message =
        "原恢复操作已由运行配置身份、来源端点状态和本地覆盖核对确认；没有重放网络命令".into();
    save_record(state, &record)?;
    record_workspace_receipt(state, &record);
    Ok(record.receipt)
}
fn record_workspace_receipt(state: &AppState, record: &Record) {
    if record.receipt.phase != RestorePhase::Confirmed
        || !record.receipt.local_saved
        || !record.receipt.runtime_confirmed
    {
        return;
    }
    let Some(identity) = &record.receipt.apply_identity else {
        return;
    };
    let Ok(config_revision) = identity.config_revision.parse() else {
        return;
    };
    state.configuration().record_transaction(
        crate::models::config_workspace::ConfigTransactionReceipt {
            transaction_id: state.configuration().next_transaction_id(),
            profile_id: record.binding.target.profile_id.clone(),
            strategy: crate::models::config_workspace::ConfigApplyStrategy::HotReload,
            state: "confirmed",
            effective_digest: record.plan.effective_digest.clone(),
            started_at_unix_ms: crate::services::common::now_unix_ms(),
            completed_at_unix_ms: crate::services::common::now_unix_ms(),
            runtime_identity: crate::models::config_workspace::ConfigRuntimeIdentityView {
                core_instance_id: identity.core_instance_id.clone(),
                config_revision,
            },
        },
    );
}
/// Only recover transactions belonging to the current ordinary source and
/// client instance. A lost apply acknowledgement can change runtime revision;
/// it must remain visible without pretending the old binding is executable.
pub(crate) fn receipts(state: &AppState) -> AppResult<Vec<RestorationReceipt>> {
    let Some(profile) = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .cloned()
    else {
        return Ok(vec![]);
    };
    let Some(source) = profile.content.clone() else {
        return Ok(vec![]);
    };
    let scope = state.client_core_snapshot().scope;
    let source_digest = digest(&source)?;
    Ok(state
        .endpoint_restoration()
        .read()?
        .records
        .into_values()
        .filter(|record| {
            record.binding.target.profile_id == profile.id
                && record.binding.source_digest == source_digest
                && record.binding.source_updated_at == profile.updated_at_unix_ms
                && record.plan.client_scope == scope
        })
        .map(|record| record.receipt)
        .collect())
}

#[cfg(test)]
mod tests;
