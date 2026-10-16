//! Host-owned, single-submission Flow control. Inventory rows grant only an
//! exact target selection; a later list never enlarges or rebinds that choice.
//! The public core command has no instance/revision preconditions. These local
//! fences do not claim atomicity against a different controller's live writes.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
    kernel::{
        configuration::BoundControl,
        zero::{parsing, queries::KernelRuntimeIdentity},
    },
    models::{core::CoreIpcOptions, flow_close::*},
    services::common::{lock, now_unix_ms},
    state::app_state::AppState,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use znet_engine_client::Subscription;
const LIMIT: usize = 500;
const BYTES: usize = 2 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(300);
const EXECUTION: Duration = Duration::from_secs(30);
const NOTICE:&str="仅关闭本次从宿主列表明确选择的连接；新连接不受影响。关闭仅以真实回执计数，已结束须有明确记录。未知结果不重放；其他程序更改运行状态后须重新核对。";
#[derive(Default)]
pub(crate) struct Owner(Mutex<Store>);
#[derive(Default)]
struct Store {
    inventories: BTreeMap<String, Collection>,
    plans: BTreeMap<String, Plan>,
    blocked: BTreeMap<(String, String), String>,
}
#[derive(Clone)]
struct Live {
    control: BoundControl,
    identity: KernelRuntimeIdentity,
    scope: ClientScope,
    generation: u64,
    session: String,
    subscription: Subscription,
}
#[derive(Clone)]
struct Collection {
    live: Arc<Live>,
    view: Inventory,
    deadline: Instant,
}
#[derive(Clone)]
struct Plan {
    live: Arc<Live>,
    view: Preview,
    deadline: Instant,
    state: &'static str,
    result: Option<ResultView>,
}
fn failure(code: &'static str, message: &str) -> AppError {
    AppError {
        code,
        message: message.into(),
        details: None,
    }
}
fn stale() -> AppError {
    failure(
        "flow_close_stale",
        "原连接列表、观察通道、配置或实例已变化；请重新查看真实列表并重新选择",
    )
}
fn unavailable() -> AppError {
    failure(
        "flow_close_unavailable",
        "尚无可核对的活动连接观察通道；请连接并刷新列表，不能猜测默认控制目标",
    )
}
fn unknown_block() -> AppError {
    failure(
        "flow_close_unknown",
        "该原连接的关闭结果未确认；仅可查看原操作，不会换计划重放",
    )
}
fn token() -> AppResult<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|_| unavailable())?;
    Ok(b.iter().map(|b| format!("{b:02x}")).collect())
}
fn opaque(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
}
fn flow_id(s: &str) -> bool {
    s.parse::<u64>().is_ok_and(|id| id.to_string() == s)
}
fn digest(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}
fn error_code(e: &AppError) -> &str {
    e.details
        .as_ref()
        .and_then(|v| v.pointer("/error/code"))
        .and_then(Value::as_str)
        .unwrap_or(e.code)
}
fn rejection(e: &AppError) -> bool {
    matches!(
        error_code(e),
        "permission_denied"
            | "invalid_argument"
            | "unsupported"
            | "feature_disabled"
            | "conflict"
            | "not_found"
    )
}
fn read_error(e: AppError) -> AppError {
    let code = match error_code(&e) {
        "permission_denied" => "permission_denied",
        "unsupported" | "feature_disabled" => "unsupported",
        "conflict" => "flow_close_stale",
        _ => "flow_close_unavailable",
    };
    failure(
        code,
        if code == "permission_denied" {
            "当前调用没有读取连接的权限；未授予关闭资格"
        } else {
            "连接读取或控制通道未确认；未授予新的关闭资格，请重新检查"
        },
    )
}
fn item(id: &str, outcome: Outcome, reason: &str) -> Item {
    Item {
        flow_id: id.into(),
        outcome,
        reason: reason.into(),
    }
}
impl Store {
    fn prune(&mut self) {
        let now = Instant::now();
        self.inventories.retain(|_, p| p.deadline > now);
        self.plans
            .retain(|_, p| p.state != "prepared" || p.deadline > now);
        while self.plans.len() >= 32 {
            let complete = self
                .plans
                .iter()
                .find(|(_, p)| {
                    p.state == "finished"
                        && p.result.as_ref().is_some_and(|r| r.counts.unknown == 0)
                })
                .map(|(id, _)| id.clone());
            if let Some(id) = complete {
                self.plans.remove(&id);
            } else {
                break;
            }
        }
    }
}
impl Live {
    fn local(&self, state: &AppState) -> AppResult<()> {
        if state.host_session_id()? != self.session || !self.subscription.is_current() {
            return Err(stale());
        }
        state
            .ensure_client_scope_generation(&self.scope, self.generation)
            .map_err(|_| stale())
    }
    async fn verify(&self, state: &AppState) -> AppResult<()> {
        self.local(state)?;
        if strict_identity(&self.control).await.map_err(|_| stale())? != self.identity {
            return Err(stale());
        }
        self.local(state)
    }
}
/// Do not use legacy string_at: a JSON number/bool is not a core identity.
async fn strict_identity(control: &BoundControl) -> AppResult<KernelRuntimeIdentity> {
    let value = control
        .call(json!({"type":"query","request":{"runtime":{}}}))
        .await
        .map_err(read_error)?;
    let runtime = value
        .get("runtime")
        .and_then(Value::as_object)
        .ok_or_else(stale)?;
    let instance = runtime
        .get("core_instance_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .ok_or_else(stale)?;
    let revision = runtime
        .get("config_revision")
        .and_then(|v| {
            v.as_u64().or_else(|| {
                let s = v.as_str()?;
                let n = s.parse::<u64>().ok()?;
                (n.to_string() == s).then_some(n)
            })
        })
        .ok_or_else(stale)?;
    Ok(KernelRuntimeIdentity {
        core_instance_id: instance.into(),
        config_revision: revision,
    })
}
fn capabilities(raw: &Value) -> AppResult<()> {
    let c = parsing::parse_capabilities(raw.get("capabilities").unwrap_or(raw), None);
    if !c.available
        || !c.contracts.as_ref().is_some_and(|c| {
            c.capabilities.minimum_supported == 1
                && c.capabilities.current >= 1
                && c.control_api.minimum_supported == 1
                && c.control_api.current >= 1
        })
        || !["runtime_snapshot", "flow_snapshot"]
            .iter()
            .all(|f| c.features.iter().any(|x| x == f))
    {
        return Err(failure(
            "unsupported",
            "当前内核未声明兼容的实例和Flow观测控制契约",
        ));
    }
    // permissions is catalog metadata, not a self-grant. The authenticated
    // local server enforces Read/Control on each actual query/command.
    Ok(())
}
fn project(raw: &Value) -> AppResult<(Target, bool)> {
    let snapshot: zero_api::FlowSnapshot = serde_json::from_value(raw.clone())
        .map_err(|_| failure("unsupported", "连接快照不符合公共Flow schema"))?;
    let r = snapshot.record.ok_or_else(|| {
        failure(
            "unsupported",
            "缺少公共Flow记录，不能用默认字段授予关闭资格",
        )
    })?;
    if !flow_id(&r.flow_id) || r.target.host.len() > 1024 || r.target.host.is_empty() {
        return Err(failure("unsupported", "连接身份或目标格式不兼容"));
    }
    let network = match r.network {
        zero_api::Network::Tcp => "tcp",
        zero_api::Network::Udp => "udp",
    };
    let key = digest(
        &json!({"flowId":r.flow_id,"startedAtUnixMs":r.timing.started_at_unix_ms.to_string(),"network":network,"inbound":r.inbound,"source":r.source.as_ref().map(|s|json!({"ip":s.ip,"port":s.port})),"target":{"host":r.target.host,"port":r.target.port}}),
    );
    let completed = r.state == zero_api::FlowState::Completed;
    Ok((
        Target {
            flow_id: r.flow_id,
            target_key: key,
            started_at_unix_ms: r.timing.started_at_unix_ms.to_string(),
            network: network.into(),
            destination: format!("{}:{}", r.target.host, r.target.port),
            can_close: !completed,
            reason: None,
        },
        completed,
    ))
}
async fn read_target(live: &Live, id: &str) -> AppResult<(Target, bool)> {
    let value = live
        .control
        .call(json!({"type":"query","request":{"flow":{"flow_id":id}}}))
        .await
        .map_err(read_error)?;
    let row = value
        .get("flow")
        .ok_or_else(|| failure("unsupported", "连接详情缺少公共响应类型"))?;
    let target = project(row)?;
    if target.0.flow_id != id {
        return Err(stale());
    }
    Ok(target)
}
fn platform() -> AppResult<()> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Err(failure(
            "unsupported",
            "移动运行宿主控制桥尚未接入，Flow关闭暂不可用",
        ))
    } else {
        Ok(())
    }
}
pub(crate) async fn inventory(state: &AppState) -> AppResult<Inventory> {
    platform()?;
    let subscription = state.observations().current().ok_or_else(unavailable)?;
    let snapshot = state.client_core_snapshot();
    if snapshot.scope_generation.0 == 0 || snapshot.scope_generation.0 > 9_007_199_254_740_991 {
        return Err(stale());
    }
    let session = state.host_session_id()?.to_owned();
    let control = BoundControl::connect(CoreIpcOptions {
        socket: Some(subscription.binding.endpoint.path.clone()),
        timeout_ms: Some(
            subscription
                .binding
                .timeout
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        ),
    })
    .await
    .map_err(read_error)?;
    let cap = control
        .call(json!({"type":"query","request":{"capabilities":{}}}))
        .await
        .map_err(read_error)?;
    capabilities(&cap)?;
    let identity = strict_identity(&control).await?;
    let live = Arc::new(Live {
        control,
        identity,
        scope: snapshot.scope,
        generation: snapshot.scope_generation.0,
        session,
        subscription,
    });
    live.local(state)?;
    let rows = live
        .control
        .call(json!({"type":"query","request":{"active_flows":{"limit":LIMIT+1,"filter":{}}}}))
        .await
        .map_err(read_error)?;
    let array = rows
        .get("active_flows")
        .and_then(Value::as_array)
        .ok_or_else(|| failure("unsupported", "活动连接缺少公共快照数组"))?;
    if array.len() > LIMIT + 1
        || serde_json::to_vec(&rows).map_err(|_| unavailable())?.len() > BYTES
    {
        return Err(failure(
            "budget_exceeded",
            "连接列表超出500项或2MiB预算；未取得关闭资格",
        ));
    }
    let truncated = array.len() > LIMIT;
    let mut targets = Vec::new();
    let mut ids = BTreeSet::new();
    for raw in array.iter().take(LIMIT) {
        let (target, completed) = project(raw)?;
        if completed || !ids.insert(target.flow_id.clone()) {
            return Err(failure("unsupported", "活动连接身份重复或状态不兼容"));
        }
        targets.push(target);
    }
    live.verify(state).await?;
    let id = token()?;
    let mut store = lock(&state.flow_closes().0, "flow_close")?;
    store.prune();
    if store.inventories.len() >= 4 {
        return Err(failure("busy", "已有四份连接选择列表，请关闭旧列表后重试"));
    }
    for target in &mut targets {
        if store.blocked.contains_key(&(
            live.identity.core_instance_id.clone(),
            target.flow_id.clone(),
        )) {
            target.can_close = false;
            target.reason = Some(unknown_block().message);
        }
    }
    let view = Inventory {
        inventory_id: id.clone(),
        core_instance_id: live.identity.core_instance_id.clone(),
        config_revision: live.identity.config_revision.to_string(),
        client_scope_generation: live.generation.to_string(),
        observed_at_unix_ms: now_unix_ms().to_string(),
        targets,
        limit: LIMIT,
        truncated,
        notice: NOTICE.into(),
    };
    store.inventories.insert(
        id,
        Collection {
            live,
            view: view.clone(),
            deadline: Instant::now() + TTL,
        },
    );
    Ok(view)
}
pub(crate) async fn preview(state: &AppState, input: PreviewInput) -> AppResult<Preview> {
    platform()?;
    if !opaque(&input.inventory_id) || input.targets.is_empty() || input.targets.len() > LIMIT {
        return Err(failure(
            "invalid_argument",
            "请从原宿主列表选择1至500项连接",
        ));
    }
    let collection = {
        let mut store = lock(&state.flow_closes().0, "flow_close")?;
        store.prune();
        store
            .inventories
            .get(&input.inventory_id)
            .cloned()
            .ok_or_else(stale)?
    };
    collection.live.verify(state).await?;
    let preview_deadline = collection.deadline.min(Instant::now() + EXECUTION);
    let mut selected = Vec::new();
    let mut ids = BTreeSet::new();
    for request in input.targets {
        let original = collection
            .view
            .targets
            .iter()
            .find(|t| t.flow_id == request.flow_id && t.target_key == request.target_key)
            .ok_or_else(stale)?;
        if !ids.insert(request.flow_id.clone()) || !original.can_close {
            return Err(stale());
        }
        let mut target = original.clone();
        let mut status = "active";
        let remaining = preview_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(failure(
                "budget_exceeded",
                "预览读取达到30秒预算；未建立新关闭计划，请重新查看列表",
            ));
        }
        let read = tokio::time::timeout(
            remaining.min(Duration::from_secs(5)),
            read_target(&collection.live, &request.flow_id),
        )
        .await
        .map_err(|_| unavailable())?;
        match read {
            Ok((current, ended)) if current.target_key == original.target_key => {
                if ended {
                    status = "ended";
                    target.can_close = false;
                    target.reason = Some("原连接已有明确完成记录，不会提交关闭".into());
                }
            }
            Ok(_) => {
                status = "unavailable";
                target.can_close = false;
                target.reason = Some("原连接身份已变化，不会关闭其他对象".into());
            }
            Err(_) => {
                status = "unavailable";
                target.can_close = false;
                target.reason = Some("原连接读取未确认；缺行或not_found不代表已关闭".into());
            }
        }
        selected.push(PreviewTarget {
            target,
            state: status,
        });
    }
    collection.live.verify(state).await?;
    if collection.deadline <= Instant::now() || preview_deadline <= Instant::now() {
        return Err(stale());
    }
    let id = token()?;
    let mut store = lock(&state.flow_closes().0, "flow_close")?;
    store.prune();
    if store.plans.len() >= 32
        || store
            .plans
            .values()
            .filter(|p| p.state == "prepared")
            .count()
            >= 4
    {
        return Err(failure(
            "busy",
            "原关闭记录或预览已达预算，请核对已有操作；不会删除未知证据",
        ));
    }
    if selected.iter().any(|t| {
        store.blocked.contains_key(&(
            collection.live.identity.core_instance_id.clone(),
            t.target.flow_id.clone(),
        ))
    }) {
        return Err(unknown_block());
    }
    let view = Preview {
        plan_id: id.clone(),
        core_instance_id: collection.live.identity.core_instance_id.clone(),
        config_revision: collection.live.identity.config_revision.to_string(),
        expires_at_unix_ms: now_unix_ms().saturating_add(300_000).to_string(),
        targets: selected,
        notice: NOTICE.into(),
    };
    store.plans.insert(
        id,
        Plan {
            live: collection.live,
            view: view.clone(),
            deadline: Instant::now() + TTL,
            state: "prepared",
            result: None,
        },
    );
    Ok(view)
}
fn acknowledged(value: &Value, id: &str) -> Outcome {
    match value.get("accepted").and_then(Value::as_bool) {
        Some(false) => Outcome::Rejected,
        Some(true) => {
            let result = value.get("result");
            if result
                .and_then(|v| v.get("flow_id"))
                .and_then(Value::as_str)
                != Some(id)
            {
                return Outcome::Unknown;
            }
            match result
                .and_then(|v| v.get("closed"))
                .and_then(Value::as_bool)
            {
                Some(true) => Outcome::Closed,
                Some(false) => Outcome::Rejected,
                None => Outcome::Unknown,
            }
        }
        None => Outcome::Unknown,
    }
}
async fn close_one(
    state: &AppState,
    plan: &Plan,
    target: &PreviewTarget,
    started: &mut bool,
) -> Item {
    let id = &target.target.flow_id;
    if plan.live.verify(state).await.is_err() {
        return item(
            id,
            Outcome::Stale,
            "原实例、观察通道或客户端代次已失效，未提交",
        );
    }
    if target.state == "ended" {
        return item(
            id,
            Outcome::Ended,
            "预览已取得原连接的明确完成记录，未提交关闭",
        );
    }
    if target.state != "active" {
        return item(
            id,
            Outcome::NotSubmitted,
            "预览没有原连接的有效控制资格，未提交",
        );
    }
    let (current, ended) = match read_target(&plan.live, id).await {
        Ok(v) => v,
        Err(_) => {
            return item(
                id,
                Outcome::NotSubmitted,
                "原连接读取未确认，未提交；缺行不计为成功",
            )
        }
    };
    if current.target_key != target.target.target_key {
        return item(id, Outcome::Stale, "连接身份已变化，未提交其他对象");
    }
    if ended {
        return item(id, Outcome::Ended, "原连接已有明确完成记录，未提交关闭");
    }
    if plan.live.verify(state).await.is_err() {
        return item(id, Outcome::Stale, "提交前原实例或代次已变化，未提交");
    }
    *started = true;
    let response = plan
        .live
        .control
        .command("flows.close", json!({"flow_id":id}))
        .await;
    let outcome = match response {
        Ok(value) => acknowledged(&value, id),
        Err(e) if rejection(&e) => Outcome::Rejected,
        Err(_) => Outcome::Unknown,
    };
    if plan.live.verify(state).await.is_err() {
        return item(
            id,
            Outcome::Unknown,
            "原请求已进入提交阶段，随后身份或回执核对中断；不会重放",
        );
    }
    let reason = match outcome {
        Outcome::Closed => "内核明确确认原连接关闭",
        Outcome::Rejected => "内核明确拒绝或返回closed:false；未计为成功",
        _ => "关闭回执缺失、格式不完整或结果未确认；不会重放",
    };
    item(id, outcome, reason)
}
pub(crate) async fn submit(state: &AppState, id: &str) -> AppResult<ResultView> {
    platform()?;
    if !opaque(id) {
        return Err(stale());
    }
    let plan = {
        let mut store = lock(&state.flow_closes().0, "flow_close")?;
        store.prune();
        let plan = store.plans.get(id).cloned().ok_or_else(stale)?;
        if plan.state != "prepared" {
            return Err(failure(
                "flow_close_already_submitted",
                "此原计划已经提交；请只读核对原操作，不会重复执行",
            ));
        }
        plan.live.local(state)?;
        if plan.view.targets.iter().any(|t| {
            store.blocked.contains_key(&(
                plan.live.identity.core_instance_id.clone(),
                t.target.flow_id.clone(),
            ))
        }) {
            return Err(unknown_block());
        }
        for t in &plan.view.targets {
            store.blocked.insert(
                (
                    plan.live.identity.core_instance_id.clone(),
                    t.target.flow_id.clone(),
                ),
                id.into(),
            );
        }
        store.plans.get_mut(id).ok_or_else(stale)?.state = "submitted";
        plan
    };
    let deadline = Instant::now() + EXECUTION;
    let mut items = Vec::new();
    for target in &plan.view.targets {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut started = false;
        let result = if remaining.is_zero() {
            item(
                &target.target.flow_id,
                Outcome::NotSubmitted,
                "达到本次30秒执行预算，未发出后续关闭",
            )
        } else {
            match tokio::time::timeout(
                remaining.min(Duration::from_secs(5)),
                close_one(state, &plan, target, &mut started),
            )
            .await
            {
                Ok(v) => v,
                Err(_) => item(
                    &target.target.flow_id,
                    if started {
                        Outcome::Unknown
                    } else {
                        Outcome::NotSubmitted
                    },
                    if started {
                        "原请求提交后超时；结果未确认，不会重放"
                    } else {
                        "读取或预检超时，未发出关闭"
                    },
                ),
            }
        };
        {
            let mut store = lock(&state.flow_closes().0, "flow_close")?;
            let key = (
                plan.live.identity.core_instance_id.clone(),
                target.target.flow_id.clone(),
            );
            if result.outcome != Outcome::Unknown
                && store.blocked.get(&key).is_some_and(|owner| owner == id)
            {
                store.blocked.remove(&key);
            }
        }
        items.push(result);
    }
    let mut counts = Counts::default();
    for i in &items {
        match i.outcome {
            Outcome::Closed => counts.closed += 1,
            Outcome::Ended => counts.ended += 1,
            Outcome::Rejected => counts.rejected += 1,
            Outcome::Unknown => counts.unknown += 1,
            Outcome::Stale => counts.stale += 1,
            Outcome::NotSubmitted => counts.not_submitted += 1,
        }
    }
    let result = ResultView {
        plan_id: id.into(),
        state: "finished",
        items,
        counts,
    };
    let mut store = lock(&state.flow_closes().0, "flow_close")?;
    let original = store.plans.get_mut(id).ok_or_else(stale)?;
    original.state = "finished";
    original.result = Some(result.clone());
    Ok(result)
}
pub(crate) fn status(state: &AppState, id: &str) -> AppResult<Status> {
    if !opaque(id) {
        return Err(stale());
    }
    let store = lock(&state.flow_closes().0, "flow_close")?;
    let p = store.plans.get(id).ok_or_else(stale)?;
    Ok(Status {
        plan_id: id.into(),
        state: p.state,
        result: p.result.clone(),
    })
}
pub(crate) fn discard(state: &AppState, id: &str) -> AppResult<()> {
    if !opaque(id) {
        return Err(stale());
    }
    let mut store = lock(&state.flow_closes().0, "flow_close")?;
    if store.plans.get(id).is_some_and(|p| p.state != "prepared") {
        return Err(failure(
            "flow_close_already_submitted",
            "原计划已经提交，关闭页面不能撤回已发出的请求",
        ));
    }
    store.plans.remove(id);
    store.inventories.remove(id);
    Ok(())
}
#[cfg(all(test, unix))]
mod tests;
