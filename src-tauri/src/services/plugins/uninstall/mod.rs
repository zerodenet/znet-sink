//! Fixed inactive-plugin retirement. This owner never sends runtime commands,
//! selects a replacement, retries an unknown delete or infers success from absence.
mod files;
mod resources;
pub(super) mod storage;
#[cfg(test)]
mod tests;
use super::*;
use crate::{
    configuration::workspace,
    models::{
        app_config::AppConfig,
        plugin_uninstall::{Item, Preview, Profile, Resource, Status},
        proxy_config::ProxyConfigProfile,
        rule_set::RuleSetProfile,
        subscription::SubscriptionProfile,
    },
    services::{
        app_config_store,
        app_database::workspace_recovery as db,
        common::{lock, now_unix_ms},
        domain_store,
        file_history::{self, Domain},
    },
    state::app_state::AppState,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::MutexGuard,
    time::{Duration, Instant},
};
use znet_plugin_sandbox::distribution::store::{UninstallReceipt, UninstallReview};
const NOTICE:&str="仅卸载本次列出的已核对插件登记、托管记录和独立数据；不切换配置或停止VPN。不可变ZRS产物、包缓存、共享主密钥和公共目录保留。逐项结果以原操作回执为准，未知不会自动重试";
const TTL: Duration = Duration::from_secs(300);
#[derive(Default)]
pub(super) struct Owner {
    plans: Mutex<BTreeMap<String, Prepared>>,
    #[cfg(test)]
    fail_at: Mutex<Option<String>>,
    #[cfg(test)]
    fail_after: Mutex<Option<String>>,
}
impl Owner {
    #[cfg(any(target_os = "android", target_os = "ios", test))]
    pub(super) fn invalidate_plans(&self) {
        self.plans
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}
#[derive(Clone)]
struct Prepared {
    native_generation: Option<String>,
    preview: Preview,
    binding: workspace::Binding,
    root_epoch: storage::Epoch,
    registration: Registration,
    store: UninstallReview,
    components: Vec<String>,
    authorities: Vec<String>,
    policies: String,
    resources: resources::ResourcePlan,
    files: Vec<files::FilePlan>,
    deadline: Instant,
}
struct Context {
    profiles: Vec<ProxyConfigProfile>,
    subs: Vec<SubscriptionProfile>,
    rules: Vec<RuleSetProfile>,
    app: AppConfig,
    binding: workspace::Binding,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    status: Status,
    origin_session: String,
    binding: String,
    resource_digest: String,
    receipts: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    operation_id: String,
    plan_id: String,
    plugin_id: String,
    package_digest: String,
    resource_ids: Vec<String>,
    kind: String,
    proof: serde_json::Value,
}
fn valid_receipt(record: &Record, item: &Item) -> bool {
    let Some(value) = record.receipts.get(&item.resource_id) else {
        return false;
    };
    let Ok(r) = serde_json::from_value::<Receipt>(value.clone()) else {
        return false;
    };
    if r.schema != 1
        || r.operation_id != record.status.operation_id
        || r.plan_id != record.status.plan_id
        || r.plugin_id != record.status.plugin_id
        || r.package_digest != record.status.package_digest
        || r.kind != item.kind
        || r.resource_ids.is_empty()
        || r.resource_ids.len() > 128
        || !r.resource_ids.contains(&item.resource_id)
        || r.resource_ids.iter().collect::<BTreeSet<_>>().len() != r.resource_ids.len()
        || r.resource_ids
            .iter()
            .any(|id| !record.status.items.iter().any(|i| &i.resource_id == id))
    {
        return false;
    }
    let p = &r.proof;
    let h = |k: &str| p[k].as_str().is_some_and(|s| storage::hex(s, 64));
    match item.kind.as_str() {
        "component" => {
            p.as_object().is_some_and(|o| o.len() == 2)
                && h("originalPolicyDigest")
                && p["retiredComponents"].as_array().is_some_and(|a| {
                    a.len() == r.resource_ids.len()
                        && a.iter().all(|v| {
                            v.as_str()
                                .is_some_and(|s| r.resource_ids.iter().any(|id| id == s))
                        })
                })
        }
        "profile" | "subscription" => {
            p.as_object().is_some_and(|o| o.len() == 4)
                && h("beforeGraph")
                && h("afterGraph")
                && p["beforeEpoch"]
                    .as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .zip(p["afterEpoch"].as_str().and_then(|s| s.parse::<i64>().ok()))
                    .is_some_and(|(b, a)| b >= 0 && a > b)
        }
        "profile_overlays" | "rule_set" => {
            serde_json::from_value::<file_history::WriteReceipt>(p.clone()).is_ok_and(|r| {
                r.owner_operation.as_deref() == Some(record.status.operation_id.as_str())
                    && storage::hex(&r.before.store_id, 32)
                    && storage::hex(&r.before.file_id, 64)
                    && r.before.epoch > 0
                    && r.before.store_id == r.after.store_id
                    && r.before.file_id == r.after.file_id
                    && r.before.epoch.checked_add(1) == Some(r.after.epoch)
            })
        }
        "package_registration" => {
            serde_json::from_value::<UninstallReceipt>(p.clone()).is_ok_and(|r| {
                r.review.plugin_id == record.status.plugin_id
                    && r.review.package_digest == record.status.package_digest
                    && storage::hex(&r.review.store_digest, 64)
                    && storage::hex(&r.review.current_sha256, 64)
                    && r.review
                        .previous_sha256
                        .as_ref()
                        .is_none_or(|s| storage::hex(s, 64))
                    && storage::hex(&r.after_digest, 64)
                    && r.review.store_epoch.checked_add(1) == Some(r.after_epoch)
                    && r.blobs_retained
            })
        }
        "plugin_record" | "namespace_file" | "namespace_directory" => {
            p.as_object().is_some_and(|o| o.len() == 3)
                && h("beforeSha256")
                && (p["afterSha256"].is_null()
                    || p["afterSha256"]
                        .as_str()
                        .is_some_and(|s| storage::hex(s, 64)))
                && p["ownedAction"] == "fixed-target-write"
                && if item.kind == "plugin_record" {
                    p["afterSha256"]
                        .as_str()
                        .is_some_and(|s| storage::hex(s, 64))
                } else {
                    p["afterSha256"].is_null()
                }
        }
        _ => false,
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    schema: u32,
    records: Vec<Record>,
}
fn preflight() -> AppError {
    AppError {
        code: "plugin_uninstall_preflight_rejected",
        message:
            "卸载尚未开始：原预览已失效、目标被引用或当前配置仍在使用；请核对来源后明确重新预览"
                .into(),
        details: None,
    }
}
fn pending() -> AppError {
    AppError {
        code: "plugin_uninstall_pending",
        message: "该插件有未解决的原卸载操作，请先查看卸载记录；不会新建计划或重放删除".into(),
        details: None,
    }
}
fn id(v: &str) -> AppResult<()> {
    if v.is_empty()
        || v.len() > 160
        || !v
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        Err(preflight())
    } else {
        Ok(())
    }
}
fn digest(v: &impl Serialize) -> AppResult<String> {
    workspace::digest(v).map_err(|_| storage::error())
}
fn ledger_path(root: &Path) -> PathBuf {
    root.join("uninstall-records.json")
}
fn read_ledger(root: &Path) -> AppResult<Ledger> {
    match std::fs::symlink_metadata(root) {
        Ok(_) => storage::ordinary(root, true)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(storage::error()),
    }
    let Some(bytes) = storage::read(&ledger_path(root), 1024 * 1024)? else {
        return Ok(Ledger {
            schema: 1,
            records: vec![],
        });
    };
    let ledger: Ledger = serde_json::from_slice(&bytes).map_err(|_| storage::error())?;
    validate_ledger(&ledger)?;
    Ok(ledger)
}
fn validate_ledger(ledger: &Ledger) -> AppResult<()> {
    let mut ids = BTreeSet::new();
    if ledger.schema != 1 || ledger.records.len() > 16 {
        return Err(storage::error());
    }
    for r in &ledger.records {
        let s = &r.status;
        if r.schema != 1
            || !storage::hex(&s.operation_id, 32)
            || !storage::hex(&s.plan_id, 32)
            || !storage::hex(&r.origin_session, 32)
            || !storage::hex(&s.package_digest, 64)
            || !storage::hex(&s.publisher_fingerprint, 64)
            || !storage::hex(&r.binding, 64)
            || !storage::hex(&r.resource_digest, 64)
            || id(&s.plugin_id).is_err()
            || !ids.insert(&s.operation_id)
            || s.items.is_empty()
            || s.items.len() > 128
            || s.can_retry
            || s.plugin_name.len() > 128
            || s.version.len() > 128
        {
            return Err(storage::error());
        }
        if ![
            "prepared",
            "executing",
            "completed",
            "residual",
            "unknown",
            "manual",
        ]
        .contains(&s.state.as_str())
        {
            return Err(storage::error());
        }
        let mut item_ids = BTreeSet::new();
        for i in &s.items {
            if i.resource_id.is_empty()
                || i.resource_id.len() > 256
                || !item_ids.insert(&i.resource_id)
                || ![
                    "pending",
                    "executing",
                    "completed",
                    "residual",
                    "unknown",
                    "manual",
                ]
                .contains(&i.state.as_str())
                || i.name.len() > 256
                || i.reason.as_ref().is_some_and(|s| s.len() > 512)
                || (i.state == "completed" && !valid_receipt(r, i))
            {
                return Err(storage::error());
            }
        }
        if digest(
            &s.items
                .iter()
                .map(|r| (&r.resource_id, &r.kind, &r.name))
                .collect::<Vec<_>>(),
        )? != r.resource_digest
        {
            return Err(storage::error());
        }
        if r.receipts.keys().any(|k| !item_ids.contains(k))
            || (s.state == "completed" && s.items.iter().any(|i| i.state != "completed"))
        {
            return Err(storage::error());
        }
    }
    Ok(())
}
fn write_ledger(root: &Path, ledger: &Ledger) -> AppResult<()> {
    validate_ledger(ledger)?;
    let bytes = serde_json::to_vec(ledger).map_err(|_| storage::error())?;
    if bytes.len() > 1024 * 1024 {
        return Err(storage::error());
    }
    storage::write(&ledger_path(root), &bytes)
}
fn record_write(root: &Path, record: &Record) -> AppResult<()> {
    let mut l = read_ledger(root)?;
    if let Some(i) = l
        .records
        .iter()
        .position(|r| r.status.operation_id == record.status.operation_id)
    {
        l.records[i] = record.clone()
    } else {
        while l.records.len() >= 16 {
            if let Some(i) = l.records.iter().position(|r| r.status.state == "completed") {
                l.records.remove(i);
            } else {
                return Err(storage::error());
            }
        }
        l.records.push(record.clone());
    }
    write_ledger(root, &l)
}
fn view(record: &Record) -> Status {
    let mut s = record.status.clone();
    if matches!(s.state.as_str(), "prepared" | "executing") {
        s.state = "unknown".into();
        s.reason = Some("原执行未取得完整持久回执；仅展示已确认项，不会重放删除".into());
        for item in &mut s.items {
            if item.state == "executing" {
                item.state = "unknown".into();
                item.reason = Some("该项目的删除结果未确认，缺失本身不算成功".into());
            }
        }
    }
    s
}
fn unresolved(root: &Path, plugin: &str) -> AppResult<bool> {
    Ok(read_ledger(root)?
        .records
        .iter()
        .any(|r| r.status.plugin_id == plugin && r.status.state != "completed"))
}
fn context(state: &AppState) -> AppResult<Context> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    let snapshot = state.client_core_snapshot();
    if snapshot.scope_generation.0 == 0 || snapshot.scope_generation.0 > 9_007_199_254_740_991 {
        return Err(preflight());
    }
    let dir = workspace::recovery::directory(state)?;
    let app_file = file_history::acquire(&dir.join("app-config.json"), Domain::AppConfig)
        .map_err(|_| storage::error())?;
    let rules_file = file_history::acquire(&dir.join("rule-sets.json"), Domain::RuleSets)
        .map_err(|_| storage::error())?;
    if app_config_store::read_current_locked(&app_file).map_err(|_| storage::error())? != *app
        || digest(&domain_store::read_rule_sets_locked(&rules_file).map_err(|_| storage::error())?)?
            != digest(&*rules)?
    {
        return Err(preflight());
    }
    let b = workspace::Binding {
        host_session_id: state.host_session_id()?.into(),
        domain_epoch: db::current_epoch(&dir)?,
        app_history: app_file.token()?,
        rules_history: rules_file.token()?,
        scope: snapshot.scope,
        generation: snapshot.scope_generation.0,
        source_digest: storage::hash(b"plugin-uninstall-metadata"),
        graph_digest: digest(&(&*profiles, &*subs))?,
        app_digest: digest(&*app)?,
        rules_digest: digest(&*rules)?,
    };
    db::with_graph_epoch(&dir, b.domain_epoch, &b.graph_digest, || Ok(()))?;
    Ok(Context {
        profiles: profiles.clone(),
        subs: subs.clone(),
        rules: rules.clone(),
        app: app.clone(),
        binding: b,
    })
}
pub(super) struct OperationGuard<'a> {
    _memory: MutexGuard<'a, ()>,
    _root: storage::RootGuard,
}
impl Host {
    pub(super) fn operation_guard(&self) -> AppResult<OperationGuard<'_>> {
        let memory = self.operation.lock().map_err(|_| storage::error())?;
        self.platform.require_foreground()?;
        let root = self.root()?;
        std::fs::create_dir_all(&root).map_err(|_| storage::error())?;
        let guard = storage::RootGuard::acquire(&root)?;
        Ok(OperationGuard {
            _memory: memory,
            _root: guard,
        })
    }
    pub(super) fn uninstall_available(&self, plugin: &str) -> AppResult<()> {
        if unresolved(&self.root()?, plugin)? {
            Err(pending())
        } else {
            Ok(())
        }
    }
    pub(super) fn ensure_installation_current(&self, plugin: &str) -> AppResult<String> {
        self.uninstall_available(plugin)?;
        let state = self.state.lock().map_err(|_| storage::error())?;
        let registration = state
            .directory
            .as_ref()
            .ok_or_else(preflight)?
            .find(plugin)
            .map_err(|_| preflight())?
            .clone();
        let expected: Vec<_> = state
            .loaded
            .values()
            .filter(|l| l.component.manifest().plugin_id == plugin)
            .map(|l| {
                (
                    l.component.manifest().component_id.clone(),
                    l.component.digest().to_string(),
                    l.installation_generation,
                    l.publisher_fingerprint.clone(),
                )
            })
            .collect();
        drop(state);
        if expected.is_empty() {
            return Err(preflight());
        }
        let (actual, package) = Store::open_existing(self.root()?)
            .map_err(|_| preflight())?
            .review_uninstall(&registration)
            .map_err(|_| preflight())?;
        if expected.len() != package.components.len()
            || expected.iter().any(|(id, digest, generation, _)| {
                *generation != actual.installation_generation
                    || !package
                        .components
                        .iter()
                        .any(|c| &c.manifest().component_id == id && c.digest() == digest)
            })
        {
            return Err(preflight());
        }
        let fingerprint = publisher_fingerprint(&registration).map_err(|_| preflight())?;
        if expected.iter().any(|(_, _, _, p)| p != &fingerprint) {
            return Err(preflight());
        }
        Ok(fingerprint)
    }
    pub(crate) fn activity(&self, plugin: &str) -> AppResult<storage::Activity> {
        let _root = storage::RootGuard::acquire(&self.root()?)?;
        let activity = storage::Activity::acquire(&self.root()?, plugin, false)?;
        self.ensure_installation_current(plugin)?;
        Ok(activity)
    }
    pub(crate) fn activity_checked(
        &self,
        manager: &Manager,
        plugin: &str,
    ) -> AppResult<storage::Activity> {
        self.platform.require_foreground()?;
        // Preserve the original authority-retirement boundary before the new
        // material gate can return early. Root reservation ends on return;
        // only the shared invocation lifetime crosses execution/await.
        let _root = storage::RootGuard::acquire(&self.root()?)?;
        self.checked_rescan(manager)?;
        self.activity(plugin)
    }
    pub(crate) fn uninstall_preview(&self, state: &AppState, plugin: &str) -> AppResult<Preview> {
        id(plugin)?;
        let _memory = self.operation.lock().map_err(|_| storage::error())?;
        let root = self.root()?;
        let guard = storage::RootGuard::acquire(&root)?;
        self.uninstall_available(plugin)?;
        let c = context(state)?;
        let registration = self
            .state
            .lock()
            .map_err(|_| storage::error())?
            .directory
            .as_ref()
            .ok_or_else(preflight)?
            .find(plugin)
            .map_err(|_| preflight())?
            .clone();
        let (store, package) = Store::open_existing(&root)
            .map_err(|_| preflight())?
            .review_uninstall(&registration)
            .map_err(|_| preflight())?;
        let publisher = publisher_fingerprint(&registration).map_err(|_| preflight())?;
        let components: Vec<String> = package
            .components
            .iter()
            .map(|c| c.manifest().component_id.clone())
            .collect();
        let (policies, authorities) = component_policy(state.capabilities(), plugin, &package)?;
        let selected =
            resources::collect(&c.profiles, &c.subs, &c.rules, &c.app, plugin, &components)?;
        let mut reasons: Vec<String> = selected
            .blockers
            .iter()
            .map(|b| {
                let name = match b.kind {
                    "profile" => c
                        .profiles
                        .iter()
                        .find(|p| p.id == b.id)
                        .map(|p| p.name.as_str()),
                    "subscription" => c
                        .subs
                        .iter()
                        .find(|p| p.id == b.id)
                        .map(|p| p.name.as_str()),
                    "rule_set" => c
                        .rules
                        .iter()
                        .find(|p| p.id == b.id)
                        .map(|p| p.name.as_str()),
                    _ => None,
                }
                .map(|s| public_text(s, 96))
                .unwrap_or_else(|| {
                    if b.id.len() <= 160
                        && !b.id.is_empty()
                        && b.id
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                    {
                        public_text(&b.id, 96)
                    } else {
                        "未匹配的记录".into()
                    }
                });
                format!("{}：{}", name, blocker_text(b.reason_code))
            })
            .collect();
        if state
            .capabilities()
            .component_snapshots()
            .iter()
            .any(|p| p.running && components.iter().any(|c| p.key == format!("{plugin}/{c}")))
        {
            reasons.push("原组件仍在退出或执行中，请等其停止后明确重新预览".into());
        }
        if storage::Activity::acquire(&root, plugin, true).is_err() {
            reasons.push("另一宿主仍持有此插件的在途执行，请先停止并等待退出".into());
        }
        let (files, extra) = files::prepare(&root, &registration, &publisher, &components)?;
        reasons.extend(extra.into_iter().map(|code| match code.as_str() {
            "namespace_unrecognized_material" => "独立目录含未识别的材料，不能递归删除".into(),
            "consent_owner_unknown" => "权限登记的发布者或组件归属无法核对".into(),
            "publisher_registration_changed" => "本地发布者登记与原已验证材料不一致".into(),
            "component_record_identity_unknown" => {
                "组件设置存在未验证组件，不能按插件名称清空".into()
            }
            "schedule_owner_unknown" => "后台任务的组件身份无法核对".into(),
            _ => "插件持久材料无法完整核对，未授予删除权限".into(),
        }));
        let mut rows = Vec::new();
        rows.push(resource(
            "package_registration",
            plugin,
            &format!("{} 安装登记（不可变包缓存保留）", registration.name),
            "verified-package",
        ));
        rows[0].dependencies = components
            .iter()
            .map(|id| format!("包内已验证组件：{}", public_text(id, 128)))
            .collect();
        for component in &authorities {
            rows.push(resource(
                "component",
                &format!("{plugin}/{component}"),
                component,
                "verified-component",
            ));
        }
        for p in &selected.profile_ids {
            let name = c
                .profiles
                .iter()
                .find(|v| &v.id == p)
                .map(|p| p.name.as_str())
                .unwrap_or("托管配置");
            rows.push(resource("profile", p, name, "exact-managed-source"));
        }
        for p in &selected.subscription_ids {
            let name = c
                .subs
                .iter()
                .find(|v| &v.id == p)
                .map(|p| p.name.as_str())
                .unwrap_or("托管订阅");
            rows.push(resource("subscription", p, name, "exact-managed-source"));
        }
        for r in &selected.rule_set_ids {
            let name = c
                .rules
                .iter()
                .find(|v| &v.id == r)
                .map(|p| p.name.as_str())
                .unwrap_or("规则集登记");
            rows.push(resource("rule_set", r, name, "exact-managed-source"));
        }
        if selected
            .profile_ids
            .iter()
            .any(|id| c.app.profile_edits.contains_key(id))
        {
            rows.push(resource(
                "profile_overlays",
                plugin,
                "已列配置的客户端覆盖",
                "exact-managed-source",
            ));
        }
        for f in &files {
            rows.push(resource(
                &f.kind,
                &f.key,
                &f.name,
                if f.namespace {
                    "publisher-namespace"
                } else {
                    "verified-component"
                },
            ));
        }
        if rows.len() > 128 {
            return Err(preflight());
        }
        dependencies(&mut rows, &selected, &c, plugin);
        if rows.iter().any(|r| r.protected_reason.is_some()) && reasons.is_empty() {
            reasons.push("原资源的依赖尚未解除，不能卸载".into());
        }
        let active = selected
            .active_profile_id
            .as_ref()
            .and_then(|id| c.profiles.iter().find(|p| &p.id == id))
            .map(|p| Profile {
                id: p.id.clone(),
                name: public_text(&p.name, 128),
            });
        let plan_id = storage::random()?;
        let epoch = guard.epoch()?;
        let binding = digest(&(
            &c.binding,
            &epoch,
            &store,
            digest(&registration)?,
            &policies,
            files
                .iter()
                .map(|f| (&f.key, &f.before))
                .collect::<Vec<_>>(),
            &selected,
        ))?;
        let preview = Preview {
            plan_id: plan_id.clone(),
            plugin_id: plugin.into(),
            plugin_name: public_text(&registration.name, 128),
            version: package.version,
            package_digest: package.digest,
            publisher_id: registration.publisher.id.clone(),
            publisher_fingerprint: publisher,
            host_session_id: c.binding.host_session_id.clone(),
            binding,
            expires_at_unix_ms: now_unix_ms().saturating_add(300_000).to_string(),
            can_execute: reasons.is_empty(),
            blocked_reasons: reasons,
            active_profile: active.clone(),
            replacement_candidates: selected
                .replacements
                .iter()
                .filter(|r| r.eligible)
                .map(|r| Profile {
                    id: r.id.clone(),
                    name: public_text(&r.name, 128),
                })
                .collect(),
            requires_manual_switch: active.is_some(),
            requires_stop: false,
            resources: rows,
            notice: NOTICE.into(),
        };
        let mut plans = self.uninstalls.plans.lock().map_err(|_| storage::error())?;
        plans.retain(|_, p| p.deadline > Instant::now());
        if plans.len() >= 4 {
            return Err(preflight());
        }
        plans.insert(
            plan_id,
            Prepared {
                native_generation: self.platform.review_generation(),
                preview: preview.clone(),
                binding: c.binding,
                root_epoch: epoch,
                registration,
                store,
                components,
                authorities,
                policies,
                resources: selected,
                files,
                deadline: Instant::now() + TTL,
            },
        );
        Ok(preview)
    }
    pub(crate) fn uninstall_discard(&self, plan: &str) -> AppResult<()> {
        self.uninstalls
            .plans
            .lock()
            .map_err(|_| storage::error())?
            .remove(plan);
        Ok(())
    }
    pub(crate) fn uninstall_status(&self, operation: &str) -> AppResult<Status> {
        if !storage::hex(operation, 32) {
            return Err(preflight());
        }
        let root = self.root()?;
        let l = read_ledger(&root)?;
        l.records
            .iter()
            .find(|r| r.status.operation_id == operation)
            .map(view)
            .ok_or_else(|| AppError::not_found("plugin_uninstall", operation))
    }
    pub(crate) fn uninstall_recent(&self) -> AppResult<Vec<Status>> {
        Ok(read_ledger(&self.root()?)?
            .records
            .iter()
            .rev()
            .map(view)
            .collect())
    }
}
fn public_text(value: &str, max: usize) -> String {
    let mut result = String::new();
    for c in value.chars().filter(|c| !c.is_control()) {
        if result.len() + c.len_utf8() > max {
            break;
        }
        result.push(c);
    }
    result
}
fn blocker_text(code: &str) -> &'static str {
    match code {
        "active_profile_requires_switch" => {
            "当前仍使用此托管配置，请先在配置管理明确切换或断开并重新核对"
        }
        "active_identity_ambiguous" => "当前配置身份不唯一，不能选择默认替代",
        "shared_resource" => "其他配置或覆盖仍引用此资源",
        "ownership_collision" | "ownership_invalid" | "identity_invalid" => {
            "资源身份或所有权无法核对，不能按名称或前缀删除"
        }
        "protected_source" | "source_uninspectable" => {
            "来源受保护或材料不可检查，不能把缺少引用信息视为无引用"
        }
        "unsupported_source" | "source_schema_invalid" => "来源格式无法在此范围安全核对",
        "metadata_invalid" | "orphan_metadata" => "客户端覆盖或孤立归属无法核对",
        "resource_budget_exceeded" => "资源范围超出此卸载计划预算，未截断目标",
        _ => "原插件材料或组件状态无法完整核对，请保留资源并先处理相关来源",
    }
}
fn dependencies(
    rows: &mut [Resource],
    selected: &resources::ResourcePlan,
    c: &Context,
    plugin: &str,
) {
    for row in rows {
        let profile_id = selected
            .profile_ids
            .iter()
            .find(|id| resource("profile", id, "", "").resource_id == row.resource_id);
        let sub_id = selected
            .subscription_ids
            .iter()
            .find(|id| resource("subscription", id, "", "").resource_id == row.resource_id);
        let rule_id = selected
            .rule_set_ids
            .iter()
            .find(|id| resource("rule_set", id, "", "").resource_id == row.resource_id);
        if let Some(profile) = profile_id {
            for sub in c
                .subs
                .iter()
                .filter(|s| s.target_proxy_config_id.as_ref() == Some(profile))
            {
                row.dependencies
                    .push(format!("关联订阅：{}", public_text(&sub.name, 96)));
            }
        }
        if let Some(sub) = sub_id {
            if let Some(profile) = c
                .subs
                .iter()
                .find(|s| &s.id == sub)
                .and_then(|s| s.target_proxy_config_id.as_ref())
                .and_then(|id| c.profiles.iter().find(|p| &p.id == id))
            {
                row.dependencies
                    .push(format!("关联配置：{}", public_text(&profile.name, 96)));
            }
            for rule in c
                .rules
                .iter()
                .filter(|r| r.managed_by_subscription_id.as_ref() == Some(sub))
            {
                row.dependencies
                    .push(format!("托管规则集：{}", public_text(&rule.name, 96)));
            }
        }
        if let Some(rule) = rule_id {
            if let Some(sub) = c
                .rules
                .iter()
                .find(|r| &r.id == rule)
                .and_then(|r| r.managed_by_subscription_id.as_ref())
                .and_then(|id| c.subs.iter().find(|s| &s.id == id))
            {
                row.dependencies
                    .push(format!("归属订阅：{}", public_text(&sub.name, 96)));
            }
        }
        if row.kind == "profile_overlays" {
            for p in c.profiles.iter().filter(|p| {
                selected.profile_ids.contains(&p.id) && c.app.profile_edits.contains_key(&p.id)
            }) {
                row.dependencies
                    .push(format!("配置覆盖：{}", public_text(&p.name, 96)));
            }
        }
        if row.kind == "component" {
            row.dependencies
                .push(format!("原已验证插件：{}", public_text(plugin, 96)));
        }
        for reference in &selected.shared_resources {
            let matches = match reference.kind {
                "profile" => profile_id.is_some_and(|id| id == &reference.id),
                "rule_set" => rule_id.is_some_and(|id| id == &reference.id),
                _ => false,
            };
            if matches {
                let owner = reference
                    .profile_id
                    .as_ref()
                    .and_then(|id| c.profiles.iter().find(|p| &p.id == id))
                    .map(|p| public_text(&p.name, 96))
                    .unwrap_or_else(|| "公共客户端覆盖".into());
                let field = reference
                    .field_path
                    .as_deref()
                    .map(|s| public_text(s, 160))
                    .unwrap_or_default();
                row.dependencies.push(format!("仍被引用：{owner} {field}"));
                row.protected_reason = Some("共享引用尚未解除，不能卸载此资源".into());
            }
        }
        for b in &selected.blockers {
            let matches = profile_id.is_some_and(|id| id == &b.id)
                || sub_id.is_some_and(|id| id == &b.id)
                || rule_id.is_some_and(|id| id == &b.id);
            if matches {
                row.protected_reason = Some(blocker_text(b.reason_code).into());
            }
        }
        if row.dependencies.len() > 128 {
            row.dependencies.truncate(128);
            row.protected_reason = Some("依赖数量超出预览预算；未授予删除权限".into());
        }
    }
}
fn resource(kind: &str, id: &str, name: &str, owner: &str) -> Resource {
    Resource {
        resource_id: format!("{kind}:{}", storage::hash(id.as_bytes())),
        kind: kind.into(),
        name: public_text(name, 128),
        ownership: owner.into(),
        dependencies: vec![],
        protected_reason: None,
    }
}
fn component_policy(
    manager: &Manager,
    plugin: &str,
    package: &znet_plugin_sandbox::distribution::package::VerifiedPackage,
) -> AppResult<(String, Vec<String>)> {
    let policies = manager.component_snapshots();
    let mut fixed = vec![];
    let mut authorities = vec![];
    for component in &package.components {
        let component_id = component.manifest().component_id.clone();
        let key = format!("{plugin}/{component_id}");
        let policy = policies.iter().find(|p| p.key == key);
        // A present retired or mismatched authority is not treated as absent
        // and cannot authorize retirement of a replacement generation.
        let identity = if let Some(p) = policy {
            if p.identity != component.digest() || p.retired {
                return Err(preflight());
            }
            authorities.push(component_id.clone());
            Some((p.registration, p.revision, p.enabled, p.running))
        } else {
            None
        };
        // No execution authority is a fixed observation only. Its signed
        // definition is removed by the real package-registration receipt;
        // no synthetic component-deletion item/receipt is manufactured.
        fixed.push((component_id, component.digest().to_owned(), identity));
    }
    Ok((digest(&fixed)?, authorities))
}

impl Host {
    /// Caller holds proxy_config_operation. The same Host/root reservation spans
    /// review, every mutation and receipt checkpoint; no nested operation mutex.
    pub(crate) fn uninstall_execute(
        &self,
        state: &AppState,
        plan: &str,
        operation: &str,
        confirmed: bool,
    ) -> AppResult<Status> {
        if !storage::hex(operation, 32) || !confirmed {
            return Err(preflight());
        }
        let _memory = self.operation.lock().map_err(|_| storage::error())?;
        let root = self.root()?;
        let root_guard = storage::RootGuard::acquire(&root)?;
        if let Some(record) = read_ledger(&root)?
            .records
            .iter()
            .find(|r| r.status.operation_id == operation)
        {
            if record.status.plan_id != plan {
                return Err(preflight());
            }
            return Ok(view(record));
        }
        let prepared = self
            .uninstalls
            .plans
            .lock()
            .map_err(|_| storage::error())?
            .get(plan)
            .cloned()
            .ok_or_else(preflight)?;
        if prepared.deadline <= Instant::now() || !prepared.preview.can_execute {
            return Err(preflight());
        }
        self.platform
            .check_review_generation(prepared.native_generation.as_deref())
            .map_err(|_| preflight())?;
        self.uninstall_available(&prepared.preview.plugin_id)?;
        let _exclusive_activity =
            storage::Activity::acquire(&root, &prepared.preview.plugin_id, true)
                .map_err(|_| preflight())?;
        let current = context(state)?;
        if digest(&current.binding)? != digest(&prepared.binding)?
            || root_guard.epoch()? != prepared.root_epoch
        {
            return Err(preflight());
        }
        let (review, package) = Store::open_existing(&root)
            .map_err(|_| preflight())?
            .review_uninstall(&prepared.registration)
            .map_err(|_| preflight())?;
        let (policies, authorities) =
            component_policy(state.capabilities(), &package.id, &package)?;
        if review != prepared.store
            || policies != prepared.policies
            || authorities != prepared.authorities
        {
            return Err(preflight());
        }
        files::verify(&root, &prepared.files)?;
        let selected = resources::collect(
            &current.profiles,
            &current.subs,
            &current.rules,
            &current.app,
            &prepared.preview.plugin_id,
            &prepared.components,
        )?;
        if digest(&selected)? != digest(&prepared.resources)?
            || !selected.blockers.is_empty()
            || selected.active_profile_id.is_some()
        {
            return Err(preflight());
        }
        // Keep full data locks in the established order through the local mutation
        // sequence. A fixed scope fence covers publication, never awaited IO.
        let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
        let mut subs = lock(state.subscriptions(), "subscription")?;
        let mut rules = lock(state.rule_sets(), "rule_set")?;
        let mut app = lock(state.app_config(), "app_config")?;
        let dir = workspace::recovery::directory(state)?;
        let mut app_file = file_history::acquire(&dir.join("app-config.json"), Domain::AppConfig)
            .map_err(|_| storage::error())?;
        let mut rules_file = file_history::acquire(&dir.join("rule-sets.json"), Domain::RuleSets)
            .map_err(|_| storage::error())?;
        app_file.require(&prepared.binding.app_history)?;
        rules_file.require(&prepared.binding.rules_history)?;
        if digest(&(&*profiles, &*subs))? != prepared.binding.graph_digest
            || digest(&*app)? != prepared.binding.app_digest
            || digest(&*rules)? != prepared.binding.rules_digest
            || app_config_store::read_current_locked(&app_file)? != *app
            || digest(&domain_store::read_rule_sets_locked(&rules_file)?)? != digest(&*rules)?
        {
            return Err(preflight());
        }
        let next_profiles: Vec<_> = profiles
            .iter()
            .filter(|p| !selected.profile_ids.contains(&p.id))
            .cloned()
            .collect();
        let next_subs: Vec<_> = subs
            .iter()
            .filter(|s| !selected.subscription_ids.contains(&s.id))
            .cloned()
            .collect();
        let next_rules: Vec<_> = rules
            .iter()
            .filter(|r| !selected.rule_set_ids.contains(&r.id))
            .cloned()
            .collect();
        let mut next_app = app.clone();
        for p in &selected.profile_ids {
            next_app.profile_edits.remove(p);
        }
        let mut record = Record {
            schema: 1,
            status: Status {
                operation_id: operation.into(),
                plan_id: plan.into(),
                plugin_id: prepared.preview.plugin_id.clone(),
                plugin_name: prepared.preview.plugin_name.clone(),
                version: prepared.preview.version.clone(),
                package_digest: prepared.preview.package_digest.clone(),
                publisher_fingerprint: prepared.preview.publisher_fingerprint.clone(),
                state: "prepared".into(),
                reason: None,
                started_at_unix_ms: Some(now_unix_ms().to_string()),
                finished_at_unix_ms: None,
                items: prepared
                    .preview
                    .resources
                    .iter()
                    .map(|r| Item {
                        resource_id: r.resource_id.clone(),
                        kind: r.kind.clone(),
                        name: public_text(&r.name, 128),
                        state: "pending".into(),
                        reason: None,
                    })
                    .collect(),
                can_retry: false,
                notice: NOTICE.into(),
            },
            origin_session: prepared.binding.host_session_id.clone(),
            binding: prepared.preview.binding.clone(),
            resource_digest: digest(
                &prepared
                    .preview
                    .resources
                    .iter()
                    .map(|r| (&r.resource_id, &r.kind, &r.name))
                    .collect::<Vec<_>>(),
            )?,
            receipts: BTreeMap::new(),
        };
        let mut graph_changed = false;
        let result=state.with_client_scope_generation(&prepared.binding.scope,prepared.binding.generation,||{
   if state.host_session_id()?!=prepared.binding.host_session_id || prepared.deadline<=Instant::now(){return Err(preflight())}
   db::with_graph_epoch(&dir,prepared.binding.domain_epoch,&prepared.binding.graph_digest,||record_write(&root,&record))?;
   self.uninstalls.plans.lock().map_err(|_|storage::error())?.remove(plan);
   record.status.state="executing".into();record_write(&root,&record)?;
   let component_items=items_of(&record,&["component"]);
   self.perform(&root,&root_guard,&mut record,&component_items,||{
    for c in &prepared.authorities{let key=format!("{}/{c}",prepared.preview.plugin_id);if !state.capabilities().remove_component(&key){return Err(preflight())}}
    self.callbacks.cancel_plugin(&prepared.preview.plugin_id);self.sensitive.clear_plugin(&prepared.preview.plugin_id);
    if state.capabilities().component_snapshots().iter().any(|p|p.running&&prepared.components.iter().any(|c|p.key==format!("{}/{c}",prepared.preview.plugin_id))){return Err(preflight())}
    {let mut loaded=self.state.lock().map_err(|_|storage::error())?;loaded.loaded.retain(|_,l|l.component.manifest().plugin_id!=prepared.preview.plugin_id);loaded.pages.remove(&prepared.preview.plugin_id);}
    Ok(serde_json::json!({"retiredComponents":component_items.clone(),"originalPolicyDigest":prepared.policies}))
   })?;
   let domain_items=items_of(&record,&["profile","subscription"]);let mut epoch=prepared.binding.domain_epoch;let mut graph=prepared.binding.graph_digest.clone();
   if !domain_items.is_empty(){self.perform(&root,&root_guard,&mut record,&domain_items,||{
    let next=db::save_domain_if_current(&dir,epoch,&graph,&next_profiles,&next_subs)?;
    let after=digest(&(&next_profiles,&next_subs))?;
    *profiles=next_profiles;*subs=next_subs;graph_changed=true;
    let receipt=serde_json::json!({"beforeEpoch":epoch.to_string(),"afterEpoch":next.to_string(),"beforeGraph":graph,"afterGraph":after});epoch=next;graph=after;Ok(receipt)
   })?;}
   // Prevent a cooperating external relationship writer from changing the graph
   // between the DB receipt and remaining fixed file removals. No nested DB work.
   db::with_graph_epoch(&dir,epoch,&graph,||{
    let app_items=items_of(&record,&["profile_overlays"]);if !app_items.is_empty(){self.perform(&root,&root_guard,&mut record,&app_items,||{
     let receipt=app_config_store::save_owned_locked(&mut app_file,&next_app,operation)?;*app=next_app;serde_json::to_value(receipt).map_err(|_|storage::error())
    })?;}
    let rule_items=items_of(&record,&["rule_set"]);if !rule_items.is_empty(){self.perform(&root,&root_guard,&mut record,&rule_items,||{
     let receipt=domain_store::save_rule_sets_owned_locked(&mut rules_file,&next_rules,operation)?;*rules=next_rules;serde_json::to_value(receipt).map_err(|_|storage::error())
    })?;}
    // Package registration first; subsequent failure remains independently
    // discoverable even when no installed component/card remains.
    let package_items=items_of(&record,&["package_registration"]);self.perform(&root,&root_guard,&mut record,&package_items,||{
     let receipt:UninstallReceipt=Store::open_existing(&root).map_err(|_|storage::error())?.uninstall_checked(&prepared.registration,&prepared.store).map_err(|_|storage::error())?;
     // Even a device/isolation-blocked package has host metadata and pages.
     // Clear those only after its actual registration-removal receipt.
     self.callbacks.cancel_plugin(&prepared.preview.plugin_id);self.sensitive.clear_plugin(&prepared.preview.plugin_id);
     {let mut loaded=self.state.lock().map_err(|_|storage::error())?;loaded.loaded.retain(|_,l|l.component.manifest().plugin_id!=prepared.preview.plugin_id);loaded.pages.remove(&prepared.preview.plugin_id);}
     serde_json::to_value(receipt).map_err(|_|storage::error())
    })?;
    for file in &prepared.files{let key=resource(&file.kind,&file.key,&file.name,"").resource_id;self.perform(&root,&root_guard,&mut record,&[key],||files::apply(&root,file))?;}
    Ok(())
   })?;
   if record.status.items.iter().any(|i|i.state!="completed"){return Err(storage::error())}
   record.status.state="completed".into();record.status.finished_at_unix_ms=Some(now_unix_ms().to_string());record.status.reason=Some("本次固定项目均取得本地回执；运行接管状态未更改，保留项见说明".into());record_write(&root,&record)?;Ok(())
  });
        let active = profiles.iter().find(|p| p.active).cloned();
        drop(rules_file);
        drop(app_file);
        drop(app);
        drop(rules);
        drop(subs);
        drop(profiles);
        if graph_changed {
            state.client_core_configuration_committed(active.as_ref());
        }
        if result.is_err() {
            // Do not publish an in-memory success if its checkpoint failed. Reload the
            // durable record: executing is projected unknown, never retried.
            let durable = read_ledger(&root)?
                .records
                .into_iter()
                .find(|r| r.status.operation_id == operation);
            if let Some(durable) = durable.as_ref() {
                return Ok(view(durable));
            }
            if result.as_ref().err().is_some_and(|error| {
                matches!(
                    error.code,
                    "conflict" | "plugin_uninstall_preflight_rejected"
                )
            }) {
                return Err(preflight());
            }
            return Err(storage::error());
        }
        Ok(record.status)
    }
    fn perform(
        &self,
        root: &Path,
        guard: &storage::RootGuard,
        record: &mut Record,
        ids: &[String],
        mutate: impl FnOnce() -> AppResult<serde_json::Value>,
    ) -> AppResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        for id in ids {
            let i = record
                .status
                .items
                .iter_mut()
                .find(|i| &i.resource_id == id)
                .ok_or_else(storage::error)?;
            if i.state != "pending" {
                return Err(storage::error());
            }
            i.state = "executing".into();
        }
        record_write(root, record)?;
        #[cfg(test)]
        if self
            .uninstalls
            .fail_at
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|kind| {
                record
                    .status
                    .items
                    .iter()
                    .any(|i| ids.contains(&i.resource_id) && &i.kind == kind)
            })
        {
            for i in record
                .status
                .items
                .iter_mut()
                .filter(|i| ids.contains(&i.resource_id))
            {
                i.state = "residual".into();
                i.reason = Some("该阶段写入前失败，原材料保留；请手动核对".into());
            }
            record.status.state = "residual".into();
            record_write(root, record)?;
            return Err(storage::error());
        }
        guard.advance()?;
        let receipt = match mutate() {
            Ok(v) => v,
            Err(_) => {
                for i in record
                    .status
                    .items
                    .iter_mut()
                    .filter(|i| ids.contains(&i.resource_id))
                {
                    i.state = "unknown".into();
                    i.reason = Some("该阶段未取得完整回执，原目标不得再次删除".into());
                }
                record.status.state = "unknown".into();
                record.status.reason =
                    Some("原卸载部分结果未确认；仅查看本次记录，不会自动重放".into());
                record_write(root, record)?;
                return Err(storage::error());
            }
        };
        #[cfg(test)]
        if self
            .uninstalls
            .fail_after
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|kind| {
                record
                    .status
                    .items
                    .iter()
                    .any(|i| ids.contains(&i.resource_id) && &i.kind == kind)
            })
        {
            return Err(storage::error());
        }
        for i in record
            .status
            .items
            .iter_mut()
            .filter(|i| ids.contains(&i.resource_id))
        {
            i.state = "completed".into();
            let envelope = Receipt {
                schema: 1,
                operation_id: record.status.operation_id.clone(),
                plan_id: record.status.plan_id.clone(),
                plugin_id: record.status.plugin_id.clone(),
                package_digest: record.status.package_digest.clone(),
                resource_ids: ids.to_vec(),
                kind: i.kind.clone(),
                proof: receipt.clone(),
            };
            record.receipts.insert(
                i.resource_id.clone(),
                serde_json::to_value(envelope).map_err(|_| storage::error())?,
            );
        }
        record_write(root, record)
    }
}
fn items_of(record: &Record, kinds: &[&str]) -> Vec<String> {
    record
        .status
        .items
        .iter()
        .filter(|i| kinds.contains(&i.kind.as_str()))
        .map(|i| i.resource_id.clone())
        .collect()
}
