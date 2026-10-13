//! Fixed, redacted completed-flow snapshots for native save/share. No UI bytes or paths.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
    models::{
        debug::{DebugFrame, DebugFramePage, DebugFrameQuery},
        native_documents::{DocumentDelivery, DocumentReceipt},
    },
    services::{
        common::{lock, now_unix_ms},
        connection_history_store, native_documents,
    },
    state::app_state::AppState,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock, Mutex,
    },
};
use tauri::{AppHandle, Manager};
use tauri_plugin_znet_native_io::{ExportOwnership, ExportPurpose, PreparedExport};
const RECORDS: usize = 2000;
const BYTES: usize = 2 * 1024 * 1024;
const TTL: u64 = 300_000;
static NEXT: AtomicU64 = AtomicU64::new(1);
static PLANS: LazyLock<Mutex<BTreeMap<String, Plan>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
struct Plan {
    scope: ClientScope,
    expires: u64,
    bytes: Option<Vec<u8>>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionExportPreview {
    pub plan_id: String,
    pub records: usize,
    pub truncated: bool,
    pub bytes: u32,
    pub expires_at_unix_ms: String,
    pub filters_fingerprint: String,
    pub omissions: Vec<&'static str>,
}
fn conflict(message: &str) -> AppError {
    AppError::conflict("connection_export", "", message)
}
fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn text<'a>(record: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| record.get(*key).and_then(Value::as_str))
}
fn number(record: &Value, keys: &[&str]) -> Value {
    keys.iter()
        .find_map(|key| {
            record.get(*key).and_then(|v| {
                v.as_u64().map(|n| n.to_string()).or_else(|| {
                    v.as_str()
                        .filter(|s| {
                            !s.is_empty()
                                && s.len() <= 20
                                && s.bytes().all(|b| b.is_ascii_digit())
                                && s.parse::<u64>().is_ok()
                        })
                        .map(str::to_owned)
                })
            })
        })
        .map(Value::String)
        .unwrap_or(Value::Null)
}
fn enum_value(value: Option<&str>, allowed: &[&str]) -> Value {
    value
        .filter(|s| allowed.contains(s))
        .map(|s| json!(s))
        .unwrap_or(Value::Null)
}
fn ip(value: Option<&str>) -> Value {
    value
        .and_then(|s| s.parse::<std::net::IpAddr>().ok())
        .map(|ip| json!(ip.to_string()))
        .unwrap_or(Value::Null)
}
fn socket(value: Option<&str>) -> Value {
    value
        .and_then(|s| s.parse::<std::net::SocketAddr>().ok())
        .map(|addr| json!(addr.to_string()))
        .unwrap_or(Value::Null)
}
fn domain(value: Option<&str>) -> Value {
    value
        .filter(|s| {
            s.len() <= 253
                && s.contains('.')
                && s.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
        })
        .map(|s| json!(s.to_ascii_lowercase()))
        .unwrap_or(Value::Null)
}
fn destination_domain(value: Option<&str>) -> Value {
    let direct = domain(value);
    if !direct.is_null() {
        return direct;
    }
    value
        .and_then(|s| s.rsplit_once(':'))
        .filter(|(_, p)| p.parse::<u16>().is_ok_and(|n| n > 0))
        .map(|(name, _)| domain(Some(name)))
        .unwrap_or(Value::Null)
}
fn flow_id(value: Option<&str>) -> Value {
    value
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        })
        .map(|s| json!(s))
        .unwrap_or(Value::Null)
}
fn project(frame: &DebugFrame) -> Value {
    let payload = frame.payload.get("payload").unwrap_or(&frame.payload);
    let record = payload.get("record").unwrap_or(payload);
    let result = record.get("result").unwrap_or(record);
    let mut projected = json!({"receivedRecordId":frame.id.to_string(),"capturedAtUnixMs":frame.at_ms.to_string(),"flowId":flow_id(text(record,&["flowId","flow_id"])),"flowIdentityDigest":text(record,&["flowId","flow_id"]).map(|s|digest(s.as_bytes())),"sourceAddress":socket(text(record,&["source"])),"sourceIp":ip(text(record,&["sourceIp","source_ip"])),"targetIp":ip(text(record,&["targetIp","target_ip"])),"targetHost":domain(text(record,&["targetHost","target_host"])),"destinationAddress":socket(text(record,&["destination"])),"destinationHost":destination_domain(text(record,&["destination"])),"remoteAddress":socket(text(record,&["remoteDestination","remote_destination"])),"protocol":enum_value(text(record,&["network","protocol"]),&["tcp","udp"]),"outcome":enum_value(text(result,&["outcome"]).or_else(||text(record,&["outcome"])),&["success","completed","failed","blocked","cancelled","closed","timeout"]),"bytesUp":number(record,&["bytesUp","bytes_up"]),"bytesDown":number(record,&["bytesDown","bytes_down"]),"startedAtUnixMs":number(record,&["startedAtUnixMs","started_at_unix_ms"]),"endedAtUnixMs":number(record,&["endedAtUnixMs","ended_at_unix_ms"]),"revision":number(record,&["revision"]),"source":"locally_received_completed_flow"});
    let fields: [(&[&str], &str); 7] = [
        (&["flowId", "flow_id"], "flowId"),
        (&["source"], "sourceAddress"),
        (&["sourceIp", "source_ip"], "sourceIp"),
        (&["targetIp", "target_ip"], "targetIp"),
        (&["targetHost", "target_host"], "targetHost"),
        (
            &["remoteDestination", "remote_destination"],
            "remoteAddress",
        ),
        (&["network", "protocol"], "protocol"),
    ];
    let mut omitted = fields
        .iter()
        .filter(|(keys, field)| {
            keys.iter()
                .any(|key| record.get(*key).is_some_and(|v| !v.is_null()))
                && projected[*field].is_null()
        })
        .count();
    if record.get("destination").is_some_and(|v| !v.is_null())
        && projected["destinationAddress"].is_null()
        && projected["destinationHost"].is_null()
    {
        omitted += 1;
    }
    projected["omittedUnvalidatedFields"] = json!(omitted);
    projected
}
fn prepare(
    query: &DebugFrameQuery,
    page: DebugFramePage,
    now: u64,
) -> AppResult<(ConnectionExportPreview, Vec<u8>)> {
    let fingerprint =
        digest(serde_json::to_vec(query).map_err(|_| AppError::invalid_argument("连接筛选无效"))?);
    let records = page
        .items
        .iter()
        .take(RECORDS)
        .map(project)
        .collect::<Vec<_>>();
    let truncated = page.has_more || page.items.len() > RECORDS;
    let omissions=vec!["仅保留经类型校验的连接ID、IP/套接字、DNS域名与数值事实；进程、路由标签、URL、自由文本错误、凭据和协议材料排除。","仅为收到并保留的完成连接记录；不是活动连接、完整时间报表或包捕获。"];
    let value = json!({"schemaVersion":"znet.redacted-connection-records.v1","createdAtUnixMs":now.to_string(),"filtersFingerprint":fingerprint,"recordLimit":RECORDS,"hasMore":truncated,"completeness":"unknown","omissions":omissions,"records":records});
    let bytes =
        serde_json::to_vec(&value).map_err(|_| AppError::internal("脱敏连接快照编码失败"))?;
    if bytes.len() > BYTES {
        return Err(AppError::invalid_argument(
            "连接快照超过2MiB预算，请缩小筛选范围",
        ));
    }
    Ok((
        ConnectionExportPreview {
            plan_id: format!(
                "connection-export-{now}-{}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            records: records.len(),
            truncated,
            bytes: bytes.len() as u32,
            expires_at_unix_ms: now.saturating_add(TTL).to_string(),
            filters_fingerprint: fingerprint,
            omissions,
        },
        bytes,
    ))
}
pub(crate) fn preview(
    state: &AppState,
    mut query: DebugFrameQuery,
) -> AppResult<ConnectionExportPreview> {
    for value in [
        &query.search,
        &query.protocol,
        &query.outbound,
        &query.outcome,
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > 2048 {
            return Err(AppError::invalid_argument("连接筛选超出输入预算"));
        }
    }
    // Page cursors cannot enlarge or turn this into an arbitrary debug export.
    query.frame_type = None;
    query.before_id = None;
    query.limit = Some(RECORDS);
    let scope = state.client_core_snapshot().scope;
    let page = crate::services::diagnostic_storage::with_storage_lock(|| {
        connection_history_store::query_page(&query)
    })?;
    let now = now_unix_ms();
    let (view, bytes) = prepare(&query, page, now)?;
    state.ensure_client_scope(Some(&scope))?;
    let mut plans = lock(&PLANS, "connection_exports")?;
    plans.retain(|_, plan| plan.expires > now);
    if plans.len() >= 4 {
        return Err(AppError::invalid_argument(
            "连接导出预览已达容量，请完成或等待旧预览过期",
        ));
    }
    plans.insert(
        view.plan_id.clone(),
        Plan {
            scope,
            expires: now.saturating_add(TTL),
            bytes: Some(bytes),
        },
    );
    Ok(view)
}
fn take_plan(
    plans: &mut BTreeMap<String, Plan>,
    id: &str,
    scope: &ClientScope,
    now: u64,
) -> AppResult<Vec<u8>> {
    let plan = plans
        .get_mut(id)
        .ok_or_else(|| conflict("连接预览不存在，请重新预览"))?;
    if &plan.scope != scope || plan.expires <= now {
        return Err(conflict("配置、实例或预览有效期已变化，请重新预览"));
    }
    plan.bytes
        .take()
        .ok_or_else(|| conflict("原快照已交给系统，请核对原操作；不会自动重放"))
}
pub(crate) fn deliver(
    app: &AppHandle,
    id: &str,
    delivery: DocumentDelivery,
) -> AppResult<DocumentReceipt> {
    let state = app.state::<AppState>();
    let scope = state.client_core_snapshot().scope;
    state.ensure_client_scope(Some(&scope))?;
    let bytes = {
        let mut plans = lock(&PLANS, "connection_exports")?;
        take_plan(&mut plans, id, &scope, now_unix_ms())?
    };
    let export = PreparedExport::new(
        ExportPurpose::RedactedDiagnostic,
        ExportOwnership::HostGenerated,
        "znet-connection-records.json",
        "application/json",
        bytes,
        false,
    )
    .map_err(|_| AppError::invalid_argument("连接导出不符合系统用途或容量策略"))?;
    native_documents::deliver_prepared(app, export, delivery)
}
#[cfg(test)]
mod tests;
