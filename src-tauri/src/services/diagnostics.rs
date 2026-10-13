//! Immutable host-prepared public diagnostics. Free text is never assumed safe.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
    models::{
        debug::{DebugFrame, DebugFrameQuery},
        logs::LogEntry,
        native_documents::{DiagnosticOmission, DiagnosticPreview, DiagnosticScope},
    },
    services::{
        common::{lock, now_unix_ms},
        debug_store,
    },
    state::app_state::AppState,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Mutex};
const EXPORT_BYTES: usize = 2 * 1024 * 1024;
const TTL: u64 = 300_000;
#[derive(Clone)]
struct Prepared {
    scope: ClientScope,
    expires: u64,
    bytes: Vec<u8>,
    submitted: bool,
}
#[derive(Default)]
pub(crate) struct DiagnosticExports {
    plans: Mutex<BTreeMap<String, Prepared>>,
}
fn conflict(message: &str) -> AppError {
    AppError::conflict("diagnostic_export", "", message)
}
fn omission(category: &str, count: u64, reason: &str) -> DiagnosticOmission {
    DiagnosticOmission {
        category: category.into(),
        count: Some(count),
        reason: reason.into(),
    }
}
fn unknown_omission(category: &str, reason: &str) -> DiagnosticOmission {
    DiagnosticOmission {
        category: category.into(),
        count: None,
        reason: reason.into(),
    }
}
/// Project only the body of a host-classified successful telemetry read/event.
/// This key filter is not a trust boundary: arbitrary payloads must never call it.
fn facts(value: &Value) -> Value {
    const NUMERIC: &[&str] = &[
        "bytes_up",
        "bytes_down",
        "total_started",
        "active_sessions",
        "completed_sessions",
        "failed_sessions",
        "blocked_sessions",
        "direct_sessions",
        "chained_sessions",
        "dropped_packets",
        "rx_dropped_packets",
        "tx_dropped_packets",
        "rx_errors",
        "tx_errors",
        "rx_bytes",
        "tx_bytes",
        "bytes_received",
        "bytes_sent",
        "uploaded_bytes",
        "downloaded_bytes",
        "packets",
        "rx_packets",
        "tx_packets",
        "dropped",
        "errors",
        "active_flows",
        "active_stream_flows",
        "active_datagram_flows",
        "active_packet_routes",
        "elapsed_ms",
        "latency_ms",
        "completed",
        "failed",
        "succeeded",
        "total",
        "count",
        "available",
        "ok",
        "accepted",
        "applied",
        "reconciled",
    ];
    const CONTAINERS: &[&str] = &[
        "data",
        "payload",
        "result",
        "snapshot",
        "stats",
        "planes",
        "counters",
        "activity",
        "traffic",
        "metrics",
        "telemetry",
        "scope",
        "scopes",
    ];
    fn visit(value: &Value, depth: usize, budget: &mut usize) -> Value {
        if depth > 5 || *budget == 0 {
            return json!({});
        }
        *budget -= 1;
        match value {
            Value::Object(object) => {
                let mut result = serde_json::Map::new();
                for (key, value) in object.iter().take(64) {
                    if crate::kernel::redaction::sensitive_key(key) {
                        continue;
                    }
                    if NUMERIC.contains(&key.as_str())
                        && (value.is_boolean()
                            || value.is_u64()
                            || value.as_str().is_some_and(|text| {
                                text.len() <= 20
                                    && !text.is_empty()
                                    && text.bytes().all(|byte| byte.is_ascii_digit())
                                    && text
                                        .parse::<u64>()
                                        .is_ok_and(|number| number.to_string() == text)
                            }))
                    {
                        result.insert(
                            key.clone(),
                            value
                                .as_u64()
                                .map_or_else(|| value.clone(), |number| json!(number.to_string())),
                        );
                    } else if matches!(key.as_str(), "plane" | "kind")
                        && value.as_str().is_some_and(|kind| {
                            [
                                "flow",
                                "inner",
                                "outer",
                                "host",
                                "global",
                                "inbound",
                                "outbound",
                                "endpoint",
                                "peer",
                                "host_interface",
                            ]
                            .contains(&kind)
                        })
                    {
                        result.insert(key.clone(), value.clone());
                    } else if CONTAINERS.contains(&key.as_str())
                        && (value.is_object() || value.is_array())
                    {
                        let child = visit(value, depth + 1, budget);
                        if child.as_object().is_some_and(|object| !object.is_empty())
                            || child.as_array().is_some_and(|array| !array.is_empty())
                        {
                            result.insert(key.clone(), child);
                        }
                    }
                }
                Value::Object(result)
            }
            Value::Array(array) => Value::Array(
                array
                    .iter()
                    .take(32)
                    .map(|value| visit(value, depth + 1, budget))
                    .filter(|value| {
                        value.as_object().is_some_and(|object| !object.is_empty())
                            || value.as_array().is_some_and(|array| !array.is_empty())
                    })
                    .collect(),
            ),
            _ => json!({}),
        }
    }
    visit(value, 0, &mut 256)
}

const HOST_MARKER: &str = "__znetHostDiagnosticV1";
pub(crate) fn strip_untrusted_marker(fields: &mut Option<Value>) {
    if let Some(Value::Object(object)) = fields {
        object.remove(HOST_MARKER);
    }
}
pub(crate) fn host_telemetry(action: &str, outcome: &str) -> Value {
    let action = match action {
        "run" => "run",
        "uninstall" => "uninstall",
        "sdk.SubscriptionApply" => "managed-subscription-apply",
        "sdk.SubscriptionMetadataUpdate" => "managed-subscription-metadata",
        "sdk.SubscriptionSyncComplete" => "managed-subscription-sync",
        "sdk.SubscriptionRemove" => "managed-subscription-remove",
        "sdk.ProtectedLoad" => "protected-material-load",
        action if action.starts_with("scheduled.") => "scheduled-component-action",
        _ => "component-action",
    };
    if !matches!(outcome, "ok" | "failed") {
        return Value::Null;
    }
    json!({"schema":"host-action-outcome.v1","action":action,"outcome":outcome})
}
fn host_facts(marker: &Value) -> Option<Value> {
    let action = marker["action"].as_str()?;
    let outcome = marker["outcome"].as_str()?;
    if marker["schema"] != "host-action-outcome.v1"
        || ![
            "run",
            "uninstall",
            "managed-subscription-apply",
            "managed-subscription-metadata",
            "managed-subscription-sync",
            "managed-subscription-remove",
            "protected-material-load",
            "scheduled-component-action",
            "component-action",
        ]
        .contains(&action)
        || !matches!(outcome, "ok" | "failed")
    {
        return None;
    }
    Some(json!({"action":action,"outcome":outcome}))
}
fn log(entry: &LogEntry) -> Value {
    let host_facts = entry
        .fields
        .as_ref()
        .and_then(|fields| fields.get(HOST_MARKER))
        .and_then(host_facts);
    json!({"id":entry.id.to_string(),"occurredAtUnixMs":entry.occurred_at_unix_ms.to_string(),"source":entry.source,"level":entry.level,"hostFacts":host_facts,"messageOmitted":true,"rawFieldsOmitted":true})
}

fn frame(entry: &DebugFrame) -> Value {
    let direction = match entry.direction.as_str() {
        "tx" => "tx",
        "rx" => "rx",
        _ => "unprovided",
    };
    let frame_type = match entry.frame_type.as_str() {
        "query" => "query",
        "response" => "response",
        "event" => "event",
        "subscribe-ack" => "subscribe-ack",
        "error" => "error",
        _ => "other",
    };
    let (telemetry_facts, omitted_reason) = match entry.telemetry {
        None => (Value::Null, Some("no_host_telemetry_provenance")),
        Some(provenance) => match provenance.body(entry) {
            None => (
                Value::Null,
                Some("not_successful_matching_telemetry_envelope"),
            ),
            Some(body) => {
                let projected = facts(body);
                if projected
                    .as_object()
                    .is_some_and(|object| object.is_empty())
                {
                    (
                        Value::Null,
                        Some("no_available_public_facts_in_bounded_copy"),
                    )
                } else {
                    (projected, None)
                }
            }
        },
    };
    let mut output = json!({"id":entry.id.to_string(),"atUnixMs":entry.at_ms.to_string(),"direction":direction,"frameType":frame_type,"elapsedMs":entry.elapsed_ms.map(|value|value.to_string()),"hadError":entry.error.is_some(),"facts":telemetry_facts,"factsOmittedReason":omitted_reason});
    if let Some(meta) = entry.payload.get("_diagnosticCapture") {
        output["capture"] = json!({"truncated":meta["truncated"].as_bool(),"detailDiscarded":meta["detailDiscarded"].as_bool()});
    }
    output
}
pub(crate) fn preview(state: &AppState) -> AppResult<DiagnosticPreview> {
    let now = now_unix_ms();
    let scope = state.client_core_snapshot().scope;
    let (logs, log_total, source_counts) = {
        let rows = lock(state.logs(), "logs")?;
        let mut counts = [0u64; 3];
        for row in rows.iter() {
            counts[match row.source {
                crate::models::logs::LogSource::App => 0,
                crate::models::logs::LogSource::Core => 1,
                crate::models::logs::LogSource::Plugin => 2,
            }] += 1;
        }
        (
            rows.iter().rev().take(1000).map(log).collect::<Vec<_>>(),
            rows.len(),
            counts,
        )
    };
    let mut omissions = vec![
        omission(
            "logMessages",
            log_total as u64,
            "自由日志正文缺少可证明不含秘密的材料分类，默认排除",
        ),
        omission(
            "rawLogFields",
            log_total as u64,
            "原始字段即使是数字也不视为可信公共事实；仅保留宿主分类的固定action/outcome",
        ),
        unknown_omission(
            "configurationAndSecrets",
            "配置正文、来源URL、headers、token、凭据、插件材料始终排除",
        ),
        unknown_omission("pcap", "这是客户端诊断记录，不提供PCAP全包捕获"),
    ];
    omissions.push(unknown_omission(
        "kernelLiveHealth",
        "该快照仅包含真实客户端/平台状态，未读取实时Proxy健康；不会用验证组件或UI布尔值代替",
    ));
    for (source, count) in ["app", "core", "plugin"].into_iter().zip(source_counts) {
        omissions.push(omission(&format!("{source}LogTextAndUnknownFields"),count,"只导出存储来源、级别、时间及宿主分类的固定action/outcome；任意字段和未经宿主确认的组件身份不导出"));
    }
    if log_total > logs.len() {
        omissions.push(omission(
            "logHistory",
            (log_total - logs.len()) as u64,
            "仅取最近1000条；历史是有界存储",
        ));
    }
    let mut frame_read = true;
    let frames = match debug_store::query_page(&DebugFrameQuery {
        limit: Some(1000),
        ..Default::default()
    }) {
        Ok(page) => {
            if page.has_more {
                omissions.push(unknown_omission(
                    "frameHistory",
                    "仅取最多512KiB有界页，未读取更早记录",
                ));
            }
            page.items.iter().map(frame).collect::<Vec<_>>()
        }
        Err(_) => {
            frame_read = false;
            omissions.push(unknown_omission(
                "framesUnavailable",
                "已存调试记录读取失败；未伪造空采样",
            ));
            vec![]
        }
    };
    omissions.push(unknown_omission(
        "storedFrameReadLimits",
        "有界读取器可能跳过超大或无法解码的旧记录，未提供其数量；帧数量只计本次成功读取的有效记录",
    ));
    omissions.push(omission(
        "rawFramePayloadAndErrors",
        frames.len() as u64,
        "仅宿主原请求绑定的统计成功回执或规范统计事件可导出公共计量；请求、配置、诊断及无可信来源的历史帧正文排除",
    ));
    omissions.push(omission(
        "unclassifiedOrUnavailableFrameFacts",
        frames.iter().filter(|frame| !frame["factsOmittedReason"].is_null()).count() as u64,
        "无宿主遥测来源、失败/不匹配回执或有界副本缺计量的帧只保留元数据；不会用任意payload自报方法授信",
    ));
    let mut tasks = Vec::new();
    for job in state
        .list_client_probe_jobs(scope.profile_id.as_ref().map(|id| id.0.clone()))
        .into_iter()
        .filter(|job| job.scope == scope)
        .take(128)
    {
        tasks.push(json!({"source":"probe","id":job.id.0.to_string(),"kind":job.kind,"state":job.state,"targetCount":job.target_tags.len(),"completed":job.completed,"succeeded":job.succeeded,"failed":job.failed,"startedAtUnixMs":job.started_at_unix_ms.to_string(),"updatedAtUnixMs":job.updated_at_unix_ms.to_string(),"deadlineAtUnixMs":job.deadline_at_unix_ms.to_string()}));
    }
    #[cfg(any(feature = "tool-dns", feature = "tool-route"))]
    for job in state
        .tool_runtime()
        .list(None)
        .into_iter()
        .filter(|job| job.scope == scope)
        .take(128)
    {
        tasks.push(json!({"source":"tool","id":job.id.0.to_string(),"kind":job.kind,"state":job.state,"hadError":job.error.is_some(),"createdAtUnixMs":job.created_at_unix_ms.to_string(),"updatedAtUnixMs":job.updated_at_unix_ms.to_string(),"deadlineAtUnixMs":job.deadline_at_unix_ms.to_string()}));
    }
    omissions.push(omission(
        "taskTargetsResultsErrors",
        tasks.len() as u64,
        "任务subject、目标标签、params、result及错误正文排除，仅导出真实队列状态与计数",
    ));
    let public_scope = DiagnosticScope {
        profile_id: scope.profile_id.as_ref().map(|id| id.0.clone()),
        config_revision: scope.config_revision.0.to_string(),
        client_instance_id: scope.core_instance_id.0.to_string(),
    };
    let document = json!({"schemaVersion":"znet.mobile-diagnostics.v2","createdAtUnixMs":now.to_string(),"appVersion":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"scope":public_scope,"platformContract":crate::mobile_host::snapshot(),"clientSourceStatus":state.client_core_snapshot().source_status,"historyScope":"bounded_client_storage_across_instances","capture":crate::models::debug::capture_status(),"logs":logs,"frames":frames,"tasks":tasks,"omissions":omissions});
    let bytes = bounded_document(&document)?;
    state.ensure_client_scope(Some(&scope))?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| AppError::internal("无法创建诊断快照标识"))?;
    let plan_id = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut plans = lock(&state.diagnostic_exports().plans, "diagnostic_exports")?;
    plans.retain(|_, plan| plan.expires > now && !plan.submitted);
    if plans.len() >= 4 {
        return Err(conflict("已有多个未交付诊断快照，请关闭旧预览后重试"));
    }
    let preview = DiagnosticPreview {
        plan_id: plan_id.clone(),
        created_at_unix_ms: now.to_string(),
        expires_at_unix_ms: now.saturating_add(TTL).to_string(),
        scope: public_scope,
        bytes: bytes.len() as u32,
        log_count: logs.len(),
        frame_count: frame_read.then_some(frames.len()),
        task_count: tasks.len(),
        omissions,
    };
    plans.insert(
        plan_id,
        Prepared {
            scope,
            expires: now.saturating_add(TTL),
            bytes,
            submitted: false,
        },
    );
    Ok(preview)
}
fn bounded_document(document: &Value) -> AppResult<Vec<u8>> {
    // Inputs are independently bounded before serialization, then a writer
    // enforces the final file budget without allocating an oversized result.
    struct Writer(Vec<u8>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > EXPORT_BYTES {
                return Err(std::io::Error::other("diagnostic export budget"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Vec::new());
    serde_json::to_writer(&mut writer, document).map_err(|_| {
        AppError::invalid_argument("脱敏诊断快照超过2MiB预算，请清理或缩小材料范围")
    })?;
    Ok(writer.0)
}
pub(crate) fn take(state: &AppState, plan_id: &str) -> AppResult<Vec<u8>> {
    let mut plans = lock(&state.diagnostic_exports().plans, "diagnostic_exports")?;
    let plan = plans
        .get_mut(plan_id)
        .ok_or_else(|| conflict("诊断预览不存在，请重新预览"))?;
    state.ensure_client_scope(Some(&plan.scope))?;
    if now_unix_ms() >= plan.expires {
        return Err(conflict("诊断预览已过期，请重新预览"));
    }
    if plan.submitted {
        return Err(conflict("原快照已交给系统；请核对原操作，不会自动重放"));
    }
    plan.submitted = true;
    Ok(std::mem::take(&mut plan.bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_text_and_unknown_nested_fields_never_export_secrets() {
        let value = json!({"message":"raw-secret","params":{"rx_bytes":123,"token":"secret"},"config":{"tx_bytes":456},"data":{"url":"https://private/token","rx_bytes":"18446744073709551615","ok":true,"other":"canary"},"error":{"rx_bytes":999}});
        let result = facts(&value);
        assert_eq!(
            result,
            json!({"data":{"rx_bytes":"18446744073709551615","ok":true}})
        );
        assert!(!result.to_string().contains("canary"));
    }
    #[test]
    fn malformed_or_large_numeric_strings_and_raw_frame_errors_are_omitted() {
        assert_eq!(
            facts(&json!({"rx_bytes":"token42","tx_bytes":"9".repeat(1000)})),
            json!({})
        );
        let value = DebugFrame {
            id: 1,
            at_ms: 2,
            direction: "malicious-canary".into(),
            frame_type: "credential-canary".into(),
            payload: json!({"rx_bytes":3,"password":"canary"}),
            telemetry: None,
            elapsed_ms: None,
            error: Some("raw-canary".into()),
        };
        assert!(!frame(&value).to_string().contains("canary"));
    }
    #[test]
    fn export_writer_rejects_oversized_files_without_fake_partial_success() {
        assert!(bounded_document(&json!({"message":"x".repeat(EXPORT_BYTES+1)})).is_err());
    }
    #[test]
    fn snapshot_is_fixed_scope_bound_and_single_delivery() {
        let state = AppState::default();
        let scope = state.client_core_snapshot().scope;
        let plan = Prepared {
            scope,
            expires: now_unix_ms() + 1000,
            bytes: b"original".to_vec(),
            submitted: false,
        };
        state
            .diagnostic_exports()
            .plans
            .lock()
            .unwrap()
            .insert("fixed".into(), plan);
        assert_eq!(take(&state, "fixed").unwrap(), b"original");
        assert!(take(&state, "fixed").is_err());
    }
    #[test]
    fn expired_or_changed_instance_preview_never_submits() {
        let state = AppState::default();
        let scope = state.client_core_snapshot().scope;
        let plan = Prepared {
            scope,
            expires: now_unix_ms() + 1000,
            bytes: b"fixed".to_vec(),
            submitted: false,
        };
        state
            .diagnostic_exports()
            .plans
            .lock()
            .unwrap()
            .insert("changed".into(), plan.clone());
        state.client_core_instance_started();
        assert!(take(&state, "changed").is_err());
        let expired = Prepared {
            scope: state.client_core_snapshot().scope,
            expires: 0,
            ..plan
        };
        state
            .diagnostic_exports()
            .plans
            .lock()
            .unwrap()
            .insert("expired".into(), expired);
        assert!(take(&state, "expired").is_err());
    }
    #[test]
    fn different_counter_planes_remain_independent_in_exported_frames() {
        let input = json!({"planes":[{"plane":"flow","counters":{"rx_bytes":"100"}},{"plane":"outer","counters":{"rx_bytes":"200"}},{"plane":"private-canary","token":"secret"}]});
        let projected = facts(&input);
        assert_eq!(projected["planes"].as_array().unwrap().len(), 2);
        assert_eq!(projected["planes"][0]["counters"]["rx_bytes"], "100");
        assert_eq!(projected["planes"][1]["counters"]["rx_bytes"], "200");
    }
    fn telemetry_frame(payload: Value) -> DebugFrame {
        DebugFrame {
            id: 1,
            at_ms: 2,
            direction: "rx".into(),
            frame_type: "query".into(),
            payload,
            telemetry: None,
            elapsed_ms: Some(3),
            error: None,
        }
    }
    #[test]
    fn requests_config_diagnostics_and_self_reported_methods_never_export_numeric_canaries() {
        use crate::models::debug::DebugTelemetry;
        for payload in [
            json!({"type":"query","request":{"stats":{"rx_bytes":87654321}}}),
            json!({"ok":true,"result":{"config":{"rx_bytes":87654321}}}),
            json!({"ok":true,"result":{"diagnostics":{"count":87654321}}}),
            json!({"ok":true,"result":{"stats":{"rx_bytes":87654321}},"method":"stats","telemetry":"query_stats"}),
            json!({"schema_id":"zero.event.v1","event_type":"stats.sampled","payload":{"rx_bytes":87654321}}),
        ] {
            let entry = telemetry_frame(payload);
            let exported = frame(&entry);
            assert!(exported["facts"].is_null());
            assert_eq!(
                exported["factsOmittedReason"],
                "no_host_telemetry_provenance"
            );
            assert!(!exported.to_string().contains("87654321"));
        }
        // Even a genuine telemetry request cannot authorize a mismatched reply,
        // a failed response, or a request body copied onto the transmit path.
        let mut entry =
            telemetry_frame(json!({"ok":true,"result":{"config":{"rx_bytes":87654321}}}));
        entry.telemetry = Some(DebugTelemetry::QueryStats);
        assert!(frame(&entry)["facts"].is_null());
        entry.payload = json!({"ok":false,"result":{"stats":{"rx_bytes":87654321}}});
        assert!(frame(&entry)["facts"].is_null());
        entry.payload = json!({"ok":true,"result":{"stats":{"rx_bytes":87654321}}});
        entry.direction = "tx".into();
        assert!(frame(&entry)["facts"].is_null());
        entry.direction = "rx".into();
        entry.frame_type = "command".into();
        assert!(frame(&entry)["facts"].is_null());
    }
    #[test]
    fn host_bound_actual_traffic_reads_and_canonical_events_preserve_scopes_and_planes() {
        use crate::models::debug::DebugTelemetry;
        let body = json!({"scopes":[{"scope":{"kind":"endpoint","endpoint_id":"private-canary"},"planes":[{"plane":"flow","counters":{"bytes_up":"18446744073709551615"}},{"plane":"outer","counters":{"rx_bytes":200}}],"activity":{"active_stream_flows":7}}],"total":1});
        let mut entry = telemetry_frame(json!({"ok":true,"result":{"traffic_stats":body}}));
        entry.telemetry =
            DebugTelemetry::from_query(&json!({"type":"query","request":{"traffic_stats":{}}}));
        let exported = frame(&entry);
        assert!(exported["factsOmittedReason"].is_null());
        assert_eq!(
            exported["facts"]["scopes"][0]["planes"][0]["counters"]["bytes_up"],
            "18446744073709551615"
        );
        assert_eq!(
            exported["facts"]["scopes"][0]["planes"][1]["counters"]["rx_bytes"],
            "200"
        );
        assert_eq!(
            exported["facts"]["scopes"][0]["activity"]["active_stream_flows"],
            "7"
        );
        assert!(!exported.to_string().contains("private-canary"));
        entry.payload = serde_json::to_value(zero_api::ApiEvent::new(
            "event-id",
            zero_api::event_type::STATS_SCOPES_SAMPLED,
            1,
            body,
        ))
        .unwrap();
        entry.telemetry = DebugTelemetry::from_event(&entry.payload);
        entry.frame_type = "event".into();
        assert_eq!(frame(&entry)["facts"], exported["facts"]);
        entry.payload["event_type"] = json!(zero_api::event_type::CONFIG_CHANGED);
        assert!(frame(&entry)["facts"].is_null());
    }
    #[test]
    fn old_persisted_frames_decode_but_have_no_payload_export_authority() {
        let entry: DebugFrame = serde_json::from_value(json!({"id":1,"atMs":2,"direction":"rx","frameType":"query","payload":{"ok":true,"result":{"stats":{"rx_bytes":87654321}}},"elapsedMs":1,"error":null})).unwrap();
        assert!(entry.telemetry.is_none());
        assert!(frame(&entry)["facts"].is_null());
    }
    #[test]
    fn guest_cannot_forge_host_classification_and_arbitrary_numeric_log_fields_are_not_public() {
        let mut fields =
            Some(json!({"rx_bytes":123456789,"__znetHostDiagnosticV1":host_telemetry("run","ok")}));
        strip_untrusted_marker(&mut fields);
        assert!(fields.as_ref().unwrap().get(HOST_MARKER).is_none());
        let entry = LogEntry {
            id: 1,
            source: crate::models::logs::LogSource::Plugin,
            level: crate::models::logs::LogLevel::Info,
            message: "canary-secret".into(),
            fields,
            occurred_at_unix_ms: 1,
        };
        let output = log(&entry);
        assert!(output["hostFacts"].is_null());
        assert!(!output.to_string().contains("123456789"));
        assert!(!output.to_string().contains("canary"));
        assert_eq!(
            host_telemetry("scheduled.secret-canary", "failed")["action"],
            "scheduled-component-action"
        );
        assert_eq!(
            host_facts(&host_telemetry("run", "ok")).unwrap(),
            json!({"action":"run","outcome":"ok"})
        );
    }
}
