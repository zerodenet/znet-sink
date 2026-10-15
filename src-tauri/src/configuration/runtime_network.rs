//! Fixed local network-setting previews. No IPC, process, TUN or proxy-retarget IO.
use super::workspace;
use crate::{
    errors::{AppError, AppResult},
    models::{
        app_config::AppConfig, proxy_config::ProxyConfigProfile, runtime_network::*,
        subscription::SubscriptionProfile,
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
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const SCOPE: &str =
    "只保存全局客户端网络覆盖，当前普通来源固定核对；不证明监听、LAN、物理网络或 VPN 已运行";
const MAX_PLANS: usize = 4;

const MAX_BYTES: usize = 16 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(300);
#[derive(Default)]
pub(crate) struct Owner {
    plans: Mutex<BTreeMap<String, Prepared>>,
    #[cfg(test)]
    fail_after_write: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    after_publish: Mutex<Option<fn(&AppState)>>,
}
struct Prepared {
    binding: workspace::Binding,
    next_app: AppConfig,
    plan: Plan,
    status: Status,
    deadline: Instant,
}
struct Context {
    profile: ProxyConfigProfile,
    app: AppConfig,
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
        "runtime_network_conflict",
        "来源、关系、设置、规则或实例已变化；草稿保留，请明确刷新后重新预览",
    )
}
fn storage() -> AppError {
    failure(
        "runtime_network_storage",
        "运行网络本地材料读取失败；草稿保留，请核对存储状态",
    )
}
fn readonly() -> AppError {
    failure(
        "runtime_network_read_only",
        "此来源不能在运行网络编辑器修改；请到来源管理或配置工作区查看原因",
    )
}
fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(conflict());
    }
    Ok(())
}
fn admission<'a>(
    profile: &ProxyConfigProfile,
    subscriptions: &[SubscriptionProfile],
) -> Option<&'a str> {
    if profile.managed_source.is_some()
        || subscriptions.iter().any(|s| {
            s.target_proxy_config_id.as_deref() == Some(&profile.id) && s.managed_source.is_some()
        })
    {
        Some("此配置由插件托管；普通运行网络编辑器不读取或修改正文，请到来源插件管理")
    } else if profile.content.is_none() {
        Some("此来源没有普通可读正文；受保护或路径材料不在此入口读取")
    } else if profile.kernel != "zero" {
        Some("此入口只支持 Zero 原生配置")
    } else {
        None
    }
}
fn legacy_unreviewed(app: &AppConfig) -> bool {
    app.overrides != Default::default()
        || app.client_edits.is_none()
            && app.profile_edits.values().any(|edits| {
                edits.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "endpoints" | "routeRules" | "ruleSetReferences"
                    )
                })
            })
}
const LEGACY_REASON: &str =
    "旧全局覆盖尚未规范化；本入口不会隐式迁移其他字段，请在现有启动或设置迁移完成后重新核对";
fn source_guard(source: &Value) -> AppResult<zero_config::RuntimeConfig> {
    let bytes = serde_json::to_vec(source).map_err(|_| storage())?;
    if bytes.len() > MAX_BYTES {
        return Err(failure(
            "runtime_network_budget",
            "来源超过16MiB预算；本入口不解析或输出正文",
        ));
    }
    // Serde only: do not compile route/rule-set/GeoIP dependencies or resolve credentials.
    serde_json::from_slice(&bytes).map_err(|_| {
        failure(
            "runtime_network_validation",
            "来源 schema 无法核对；原材料保留，不读取依赖文件",
        )
    })
}
fn token(context: &Context) -> AppResult<String> {
    workspace::digest(&(&context.profile.id, &context.binding))
}
fn context(state: &AppState, id: &str) -> AppResult<Context> {
    validate_id(id)?;
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let profile = profiles
        .iter()
        .find(|p| p.id == id && p.active)
        .ok_or_else(conflict)?;
    // No source clone, projection or temporary DTO before source admission.
    if admission(profile, &subs).is_some() {
        return Err(readonly());
    }
    workspace::ensure_unblocked(state, id)?;
    source_guard(profile.content.as_ref().ok_or_else(readonly)?)?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let app = lock(state.app_config(), "app_config")?;
    if legacy_unreviewed(&app) {
        return Err(failure("runtime_network_read_only", LEGACY_REASON));
    }
    let snapshot = state.client_core_snapshot();
    if snapshot.scope.profile_id.as_ref().map(|id| id.0.as_str()) != Some(id) {
        return Err(conflict());
    }
    if snapshot.scope_generation.0 == 0 || snapshot.scope_generation.0 > 9_007_199_254_740_991 {
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
        scope: snapshot.scope,
        generation: snapshot.scope_generation.0,
        source_digest: workspace::digest(profile)?,
        graph_digest: workspace::digest(&(&*profiles, &*subs))?,
        app_digest: workspace::digest(&*app)?,
        rules_digest: workspace::digest(&*rules)?,
    };
    Ok(Context {
        profile: profile.clone(),
        app: app.clone(),
        binding,
    })
}

const KEYS: [&str; 4] = [
    "tun.mtu",
    "runtime.udpUpstreamIdleTimeoutSeconds",
    "localProxy.host",
    "localProxy.port",
];
fn edited<'a>(app: &'a AppConfig, key: &str) -> Option<&'a Value> {
    app.client_edits.as_ref().and_then(|v| v.get(key))
}
fn exposure(host: &str) -> AppResult<&'static str> {
    let mut ip: std::net::IpAddr = host.parse().map_err(|_| {
        failure(
            "runtime_network_validation",
            "监听地址必须是有效 IP 地址；不会解析主机名或检查网络",
        )
    })?;
    if let std::net::IpAddr::V6(v6) = ip {
        if let Some(v4) = v6.to_ipv4_mapped() {
            ip = v4.into();
        }
    }
    if ip.is_multicast() {
        return Err(failure(
            "runtime_network_validation",
            "监听地址不能是组播地址",
        ));
    }
    Ok(if ip.is_loopback() {
        "loopback"
    } else if ip.is_unspecified() {
        "all-interfaces"
    } else {
        "specific-address"
    })
}
fn values(app: &AppConfig, source: &Value) -> AppResult<Values> {
    let native = source_guard(source)?;
    let mtu = super::preferences::effective_tun_mtu(app, source)?;
    let mtu_origin = if edited(app, "tun.mtu").is_some() {
        "client"
    } else if source
        .pointer("/runtime/tun/mtu")
        .is_some_and(|v| !v.is_null())
    {
        "source-tun"
    } else if source.pointer("/runtime/network/mtu").is_some() {
        "source-network"
    } else {
        "default"
    };
    let udp = edited(app, "runtime.udpUpstreamIdleTimeoutSeconds")
        .map(|v| v.as_u64().ok_or_else(conflict))
        .transpose()?
        .unwrap_or(native.runtime.udp_upstream_idle_timeout_seconds);
    if udp == 0 {
        return Err(failure(
            "runtime_network_validation",
            "UDP 上游空闲时间必须是正整数秒",
        ));
    }
    let original = native.inbounds.iter().find(|v| {
        matches!(
            v.protocol,
            zero_config::InboundProtocolConfig::Mixed { .. }
                | zero_config::InboundProtocolConfig::HttpConnect
                | zero_config::InboundProtocolConfig::Socks5 { .. }
        )
    });
    let listener = original
        .map(|v| {
            let host = edited(app, "localProxy.host")
                .map(|v| v.as_str().ok_or_else(conflict))
                .transpose()?
                .unwrap_or(&v.listen.address)
                .to_owned();
            let port = edited(app, "localProxy.port")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u16::try_from(n).ok())
                        .ok_or_else(conflict)
                })
                .transpose()?
                .unwrap_or(v.listen.port);
            if port == 0 {
                return Err(failure(
                    "runtime_network_validation",
                    "监听端口必须在1到65535之间",
                ));
            }
            let protocol = match v.protocol {
                zero_config::InboundProtocolConfig::Mixed { .. } => "mixed",
                zero_config::InboundProtocolConfig::HttpConnect => "http",
                _ => "socks5",
            };
            Ok(Listener {
                tag: v.tag.clone(),
                protocol: protocol.into(),
                exposure: exposure(&host)?,
                host,
                port,
            })
        })
        .transpose()?;
    Ok(Values {
        mtu,
        mtu_origin,
        udp_idle_seconds: udp.to_string(),
        udp_origin: if edited(app, "runtime.udpUpstreamIdleTimeoutSeconds").is_some() {
            "client"
        } else if source
            .pointer("/runtime/udp_upstream_idle_timeout_seconds")
            .is_some()
        {
            "source"
        } else {
            "default"
        },
        listener_reason: listener
            .is_none()
            .then(|| "原配置未声明 mixed/http/socks5 本地监听；本入口不会创建监听器".into()),
        listener,
    })
}
fn snapshot_from(current: Context) -> AppResult<Snapshot> {
    let source = current.profile.content.as_ref().ok_or_else(readonly)?;
    let native = source_guard(source)?;
    let dial_leaves = native
        .outbounds
        .iter()
        .filter(|v| matches!(v.protocol, zero_config::OutboundProtocolConfig::Direct))
        .take(1024)
        .map(|v| DialLeaf {
            tag: v.tag.clone(),
            address_family: match v.dial.address_family {
                zero_config::OutboundAddressFamily::Auto => "auto",
                zero_config::OutboundAddressFamily::OnlyIpv4 => "only_ipv4",
                zero_config::OutboundAddressFamily::OnlyIpv6 => "only_ipv6",
            },
            interface: v.dial.interface.clone(),
            source_ip: v.dial.source_ip.map(|a| a.to_string()),
        })
        .collect();
    if native.outbounds.len() > 1024 {
        return Err(failure(
            "runtime_network_budget",
            "出口数量超出此页面预算；请使用工作区",
        ));
    }
    Ok(Snapshot {
        profile_id: Some(current.profile.id.clone()),
        profile_name: Some(current.profile.name.clone()),
        source_updated_at_unix_ms: Some(current.profile.updated_at_unix_ms.to_string()),
        binding: Some(token(&current)?),
        read_only_reason: None,
        values: Some(values(&current.app, source)?),
        edited_fields: KEYS
            .iter()
            .filter(|k| edited(&current.app, k).is_some())
            .map(|k| (*k).into())
            .collect(),
        dial_leaves,
        pending_apply: true,
        notice: SCOPE.into(),
    })
}
pub(crate) fn snapshot(state: &AppState) -> AppResult<Snapshot> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let profile = profiles.iter().find(|p| p.active);
    let reason = match profile {
        None => Some("请先选择一份普通配置；当前没有活动来源"),
        Some(p) => admission(p, &subs),
    };
    let reason = if reason.is_none() && legacy_unreviewed(&*lock(state.app_config(), "app_config")?)
    {
        Some(LEGACY_REASON)
    } else {
        reason
    };
    if let Some(reason) = reason {
        return Ok(Snapshot {
            profile_id: profile.map(|p| p.id.clone()),
            profile_name: profile.map(|p| p.name.clone()),
            source_updated_at_unix_ms: profile.map(|p| p.updated_at_unix_ms.to_string()),
            binding: None,
            read_only_reason: Some(reason.into()),
            values: None,
            edited_fields: vec![],
            dial_leaves: vec![],
            pending_apply: true,
            notice: SCOPE.into(),
        });
    }
    let id = profile.ok_or_else(conflict)?.id.clone();
    drop(subs);
    drop(profiles);
    snapshot_from(context(state, &id)?)
}
fn ensure_unblocked(owner: &Owner) -> AppResult<()> {
    if lock(&owner.plans, "runtime_network")?
        .values()
        .any(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "runtime_network_unknown",
            "原网络设置保存未确认；只能核对原操作，不能建立替代提交",
        ));
    }
    Ok(())
}
fn random_id() -> AppResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| storage())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn normalize(current: &Context, input: &PreviewInput) -> AppResult<AppConfig> {
    if input.changes.len() > 4
        || input.reset.len() > 4
        || input.changes.is_empty() && input.reset.is_empty()
    {
        return Err(conflict());
    }
    let mut changes = input.changes.clone();
    let mut seen = std::collections::BTreeSet::new();
    for key in input.reset.iter().chain(changes.keys()) {
        if !KEYS.contains(&key.as_str()) || !seen.insert(key) {
            return Err(failure(
                "runtime_network_validation",
                "网络字段或恢复范围不受支持，或有重复与冲突",
            ));
        }
    }
    let original = values(
        &current.app,
        current.profile.content.as_ref().ok_or_else(readonly)?,
    )?;
    if original.listener.is_none()
        && input
            .reset
            .iter()
            .chain(changes.keys())
            .any(|k| k.starts_with("localProxy."))
    {
        return Err(failure(
            "runtime_network_validation",
            "原配置没有可修改的本地监听；不会新建监听器",
        ));
    }
    for (key, value) in &mut changes {
        match key.as_str() {
            "runtime.udpUpstreamIdleTimeoutSeconds" => {
                let text = value.as_str().ok_or_else(conflict)?;
                if text.is_empty()
                    || text.len() > 20
                    || !text.bytes().all(|v| v.is_ascii_digit())
                    || text.starts_with('0')
                {
                    return Err(failure(
                        "runtime_network_validation",
                        "UDP 时间必须是正u64十进制字符串",
                    ));
                }
                let n = text
                    .parse::<u64>()
                    .map_err(|_| failure("runtime_network_validation", "UDP 时间超出u64范围"))?;
                *value = json!(n);
            }
            "localProxy.host" => {
                let host = value.as_str().ok_or_else(conflict)?;
                if host.len() > 128 || host != host.trim() {
                    return Err(conflict());
                }
                exposure(host)?;
            }
            "localProxy.port" => {
                if !value.as_u64().is_some_and(|p| p > 0 && p <= 65535) {
                    return Err(failure(
                        "runtime_network_validation",
                        "监听端口必须在1到65535之间",
                    ));
                }
            }
            "tun.mtu" => {
                if !value.as_u64().is_some_and(|m| (576..=65535).contains(&m)) {
                    return Err(failure(
                        "runtime_network_validation",
                        "MTU 必须是576到65535之间的整数",
                    ));
                }
            }
            _ => return Err(conflict()),
        }
    }
    // This domain never normalizes or migrates unrelated legacy preferences.
    let mut next = current.app.clone();
    let edits = next.client_edits.get_or_insert_with(Default::default);
    for key in &input.reset {
        edits.remove(key);
    }
    for (key, value) in changes {
        edits.insert(key, value);
    }
    if next == current.app {
        return Err(failure(
            "runtime_network_validation",
            "没有需要保存或恢复的网络覆盖",
        ));
    }
    Ok(next)
}
pub(crate) fn preview(state: &AppState, input: PreviewInput) -> AppResult<Plan> {
    ensure_unblocked(state.runtime_network())?;
    let current = context(state, &input.profile_id)?;
    if token(&current)? != input.binding {
        return Err(conflict());
    }
    let source = current.profile.content.as_ref().ok_or_else(readonly)?;
    let before = values(&current.app, source)?;
    let next = normalize(&current, &input)?;
    let after = values(&next, source)?;
    if token(&context(state, &input.profile_id)?)? != input.binding {
        return Err(conflict());
    }
    let id = random_id()?;
    let plan=Plan{plan_id:id.clone(),profile_id:input.profile_id,before,lan_exposure_warning:after.listener.as_ref().is_some_and(|l|l.exposure!="loopback")&&(input.changes.keys().chain(input.reset.iter()).any(|k|k.starts_with("localProxy."))),after,changed_fields:input.changes.keys().cloned().collect(),reset_fields:input.reset,expires_at_unix_ms:now_unix_ms().saturating_add(TTL.as_millis()as u64).to_string(),state:"prepared",notice:"固定本次全局网络覆盖范围；仅本地保存待应用，来源、运行内核、系统代理和 VPN 不变。非回环监听可能向网络暴露代理，实际绑定与设备权限尚未验证".into()};
    let mut plans = lock(&state.runtime_network().plans, "runtime_network")?;
    if plans
        .values()
        .any(|p| p.status.state == "submitted_unknown")
    {
        return Err(conflict());
    }
    plans.retain(|_, p| p.status.state == "submitted_unknown" || p.deadline > Instant::now());
    while plans.len() >= MAX_PLANS {
        let evict = plans
            .iter()
            .find(|(_, p)| matches!(p.status.state, "saved_pending_apply" | "rejected"))
            .map(|(id, _)| id.clone())
            .ok_or_else(|| {
                failure(
                    "runtime_network_busy",
                    "已有四个网络预览或未确认保存；请关闭原预览后继续",
                )
            })?;
        plans.remove(&evict);
    }
    plans.insert(
        id.clone(),
        Prepared {
            binding: current.binding,
            next_app: next,
            plan: plan.clone(),
            status: Status {
                plan_id: id,
                profile_id: plan.profile_id.clone(),
                state: "prepared",
                reason: plan.notice.clone(),
                snapshot: None,
            },
            deadline: Instant::now() + TTL,
        },
    );
    Ok(plan)
}
pub(crate) fn status(state: &AppState, id: &str) -> AppResult<Status> {
    lock(&state.runtime_network().plans, "runtime_network")?
        .get(id)
        .map(|p| p.status.clone())
        .ok_or_else(conflict)
}
pub(crate) fn discard(state: &AppState, id: &str) -> AppResult<()> {
    let mut plans = lock(&state.runtime_network().plans, "runtime_network")?;
    if plans
        .get(id)
        .is_some_and(|p| p.status.state == "submitted_unknown")
    {
        return Err(failure(
            "runtime_network_unknown",
            "原保存未确认，不能丢弃或重放",
        ));
    }
    plans.remove(id);
    Ok(())
}
/// Caller owns proxy_config_operation. Locks never span an await.
pub(crate) fn save(state: &AppState, id: &str) -> AppResult<Status> {
    let (binding, next, profile_id) = {
        let plans = lock(&state.runtime_network().plans, "runtime_network")?;
        let p = plans.get(id).ok_or_else(conflict)?;
        if p.status.state != "prepared" {
            return Ok(p.status.clone());
        }
        if p.deadline <= Instant::now() {
            return Err(failure(
                "runtime_network_expired",
                "网络预览已过期；请重新预览",
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
            p.next_app.clone(),
            p.plan.profile_id.clone(),
        )
    };
    let result = publish(state, id, &profile_id, &binding, next);
    let mut plans = lock(&state.runtime_network().plans, "runtime_network")?;
    let p = plans.get_mut(id).ok_or_else(conflict)?;
    match result {
        Ok(view) => {
            p.status.state = "saved_pending_apply";
            p.status.reason = "客户端网络覆盖已保存，运行尚未应用；请进入工作区预检和应用".into();
            p.status.snapshot = Some(view);
        }
        Err(error) => {
            if p.status.state != "submitted_unknown" {
                p.status.state = "rejected";
                p.status.reason = error.message;
            } else {
                p.status.reason =
                    "本地网络保存已进入提交，但结果未确认；保留原计划，只读核对，不会自动重放"
                        .into();
            }
        }
    }
    Ok(p.status.clone())
}
fn publish(
    state: &AppState,
    id: &str,
    profile_id: &str,
    binding: &workspace::Binding,
    next: AppConfig,
) -> AppResult<Snapshot> {
    let next_digest = workspace::digest(&next)?;
    let current = context(state, profile_id)?;
    if workspace::digest(&current.binding)? != workspace::digest(binding)? {
        return Err(conflict());
    }
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let subs = lock(state.subscriptions(), "subscription")?;
    let rules = lock(state.rule_sets(), "rule_set")?;
    let mut app = lock(state.app_config(), "app_config")?;
    if workspace::digest(&(&*profiles, &*subs))? != binding.graph_digest
        || workspace::digest(&*rules)? != binding.rules_digest
        || workspace::digest(&*app)? != binding.app_digest
    {
        return Err(conflict());
    }
    let profile = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(conflict)?;
    if admission(profile, &subs).is_some() {
        return Err(readonly());
    }
    let active = profiles.iter().find(|p| p.active).cloned();
    if !profile.active
        || binding.scope.profile_id.as_ref().map(|p| p.0.as_str()) != Some(profile_id)
    {
        return Err(conflict());
    }
    let dir = workspace::recovery::directory(state)?;
    let (mut app_file, _rules_file) = workspace::recovery::guard_files(
        &dir,
        &binding.app_history,
        &binding.rules_history,
        &binding.app_digest,
        &binding.rules_digest,
    )
    .map_err(|_| conflict())?;
    let written_history =
        state.with_client_scope_generation(&binding.scope, binding.generation, || {
            if state.host_session_id()? != binding.host_session_id {
                return Err(conflict());
            }
            workspace_recovery::with_graph_epoch(
                &dir,
                binding.domain_epoch,
                &binding.graph_digest,
                || {
                    {
                        let mut plans = lock(&state.runtime_network().plans, "runtime_network")?;
                        let plan = plans.get_mut(id).ok_or_else(conflict)?;
                        if plan.status.state != "prepared" || plan.deadline <= Instant::now() {
                            return Err(conflict());
                        }
                        plan.status.state = "submitted_unknown";
                    }
                    let receipt = app_config_store::save_owned_locked(&mut app_file, &next, id)
                        .map_err(|_| storage())?;
                    #[cfg(test)]
                    if state
                        .runtime_network()
                        .fail_after_write
                        .swap(false, std::sync::atomic::Ordering::SeqCst)
                    {
                        return Err(storage());
                    }
                    *app = next;
                    Ok(receipt.after)
                },
            )
        })?;
    drop(_rules_file);
    drop(app_file);
    drop(app);
    drop(rules);
    drop(subs);
    drop(profiles);
    if active.as_ref().is_some_and(|p| p.id == profile_id) {
        state.client_core_configuration_committed(active.as_ref());
    }
    #[cfg(test)]
    {
        let hook = lock(
            &state.runtime_network().after_publish,
            "runtime_network_test_hook",
        )?
        .take();
        if let Some(run) = hook {
            run(state);
        }
    }
    // A subsequent writer's snapshot is not the original operation's receipt.
    // Check exact publication facts before delivering an immutable local view.
    let after = context(state, profile_id)?;
    let expected_generation = binding.generation.saturating_add(u64::from(
        active.as_ref().is_some_and(|p| p.id == profile_id),
    ));
    if after.binding.app_digest != next_digest
        || after.binding.app_history != written_history
        || after.binding.rules_history != binding.rules_history
        || after.binding.source_digest != binding.source_digest
        || after.binding.graph_digest != binding.graph_digest
        || after.binding.domain_epoch != binding.domain_epoch
        || after.binding.rules_digest != binding.rules_digest
        || after.binding.scope != binding.scope
        || after.binding.generation != expected_generation
        || after.binding.host_session_id != binding.host_session_id
    {
        return Err(conflict());
    }
    snapshot_from(after)
}

#[cfg(test)]
mod tests;
