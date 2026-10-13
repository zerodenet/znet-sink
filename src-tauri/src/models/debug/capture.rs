//! Bounded, opt-in IPC detail capture. Never serialize or clone the original tree.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    LazyLock, Mutex,
};
const CAPTURE_DURATION_MS: u64 = 300_000;
const MIB: u64 = 1024 * 1024;
static CLEANING: AtomicBool = AtomicBool::new(false);
static QUEUE_LOSSES: AtomicU64 = AtomicU64::new(0);
static SESSION: LazyLock<Mutex<Session>> = LazyLock::new(|| Mutex::new(Session::default()));
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub detailed: bool,
    pub expires_at_ms: u64,
    pub session_id: Option<String>,
    pub started_at_ms: u64,
    pub ended_at_ms: Option<u64>,
    pub duration_seconds: u32,
    pub maximum_bytes: u64,
    pub stored_bytes: u64,
    pub stored_frames: u64,
    pub queue_drops_observed: u64,
    pub state: &'static str,
    pub end_reason: Option<&'static str>,
    pub scope: Option<ClientScope>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureRequest {
    pub action: String,
    pub duration_seconds: Option<u32>,
    pub capacity_mi_b: Option<u32>,
    pub expected_session_id: Option<String>,
}
#[derive(Default)]
struct Session {
    loss_baseline: u64,
    storage_epoch: u64,
    deadline: Option<std::time::Instant>,
    status: Option<CaptureStatus>,
}
impl Session {
    fn expire(&mut self, now: u64) {
        if let Some(status) = &mut self.status {
            if status.detailed {
                status.queue_drops_observed = QUEUE_LOSSES
                    .load(Ordering::Relaxed)
                    .saturating_sub(self.loss_baseline);
            }
            if status.detailed
                && (now >= status.expires_at_ms
                    || self
                        .deadline
                        .is_some_and(|deadline| std::time::Instant::now() >= deadline))
            {
                status.detailed = false;
                status.state = "stopped";
                status.end_reason = Some("duration");
                status.ended_at_ms = Some(status.expires_at_ms);
            }
        }
    }
    fn view(&mut self, now: u64) -> CaptureStatus {
        self.expire(now);
        self.status.clone().unwrap_or(CaptureStatus {
            detailed: false,
            expires_at_ms: 0,
            session_id: None,
            started_at_ms: 0,
            ended_at_ms: None,
            duration_seconds: 0,
            maximum_bytes: 0,
            stored_bytes: 0,
            stored_frames: 0,
            queue_drops_observed: 0,
            state: "idle",
            end_reason: None,
            scope: None,
        })
    }
    fn start(
        &mut self,
        duration: u32,
        capacity: u32,
        scope: Option<ClientScope>,
        now: u64,
    ) -> AppResult<CaptureStatus> {
        if !(1..=300).contains(&duration) || !(1..=10).contains(&capacity) {
            return Err(AppError::invalid_argument(
                "捕获时长须为1–300秒，容量须为1–10MiB整数",
            ));
        }
        if self.view(now).detailed {
            return Err(AppError::conflict(
                "diagnostic_capture",
                "",
                "已有捕获执行中，请先停止或核对原会话",
            ));
        }
        self.loss_baseline = QUEUE_LOSSES.load(Ordering::Relaxed);
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| AppError::internal("无法生成捕获会话标识"))?;
        let id = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.deadline =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(u64::from(duration)));
        self.status = Some(CaptureStatus {
            detailed: true,
            expires_at_ms: now.saturating_add(u64::from(duration) * 1000),
            session_id: Some(format!("capture-{id}")),
            started_at_ms: now,
            ended_at_ms: None,
            duration_seconds: duration,
            maximum_bytes: u64::from(capacity) * MIB,
            stored_bytes: 0,
            stored_frames: 0,
            queue_drops_observed: 0,
            state: "capturing",
            end_reason: None,
            scope,
        });
        Ok(self.view(now))
    }
    fn stop(&mut self, reason: &'static str, now: u64) {
        self.expire(now);
        if let Some(status) = &mut self.status {
            if status.detailed {
                status.detailed = false;
                status.state = "stopped";
                status.end_reason = Some(reason);
                status.ended_at_ms = Some(now);
            }
        }
    }
    fn invalidate(&mut self, scope: &ClientScope, now: u64) {
        if self
            .status
            .as_ref()
            .and_then(|status| status.scope.as_ref())
            .is_some_and(|original| original != scope)
        {
            self.stop("scope_changed", now);
        }
    }
    fn stop_expected(&mut self, expected: Option<&str>, now: u64) -> AppResult<CaptureStatus> {
        if expected != self.view(now).session_id.as_deref() || expected.is_none() {
            return Err(AppError::conflict(
                "diagnostic_capture",
                "",
                "捕获会话已变化，请重新核对",
            ));
        }
        self.stop("user", now);
        Ok(self.view(now))
    }
    fn admit(&mut self, id: &str, bytes: u64, now: u64) -> bool {
        self.expire(now);
        let Some(status) = &self.status else {
            return false;
        };
        if !status.detailed || status.session_id.as_deref() != Some(id) {
            return false;
        }
        if status.stored_bytes.saturating_add(bytes) > status.maximum_bytes {
            self.stop("capacity", now);
            return false;
        }
        true
    }
    fn written(&mut self, id: &str, bytes: u64, ok: bool, now: u64) {
        if let Some(status) = &mut self.status {
            if status.session_id.as_deref() != Some(id) {
                return;
            }
            if ok {
                status.stored_bytes = status.stored_bytes.saturating_add(bytes);
                status.stored_frames = status.stored_frames.saturating_add(1);
            }
            if !ok || status.stored_bytes >= status.maximum_bytes {
                self.stop(if ok { "capacity" } else { "storage_failure" }, now);
            }
        }
    }
}
fn session() -> std::sync::MutexGuard<'static, Session> {
    SESSION.lock().unwrap_or_else(|error| error.into_inner())
}
pub(super) fn queue_loss() {
    QUEUE_LOSSES.fetch_add(1, Ordering::Relaxed);
}
pub(crate) fn capture_status() -> CaptureStatus {
    session().view(crate::services::common::now_unix_ms())
}
pub(crate) fn invalidate_capture_scope(scope: &ClientScope) {
    session().invalidate(scope, crate::services::common::now_unix_ms());
}

pub(crate) fn capture_request(
    request: CaptureRequest,
    scope: ClientScope,
) -> AppResult<CaptureStatus> {
    let now = crate::services::common::now_unix_ms();
    let mut current = session();
    match request.action.as_str() {
        "start" if CLEANING.load(Ordering::Acquire) => Err(AppError::conflict(
            "diagnostic_capture",
            "",
            "正在协调捕获清理，请稍后再开始",
        )),
        "start" => current.start(
            request
                .duration_seconds
                .ok_or_else(|| AppError::invalid_argument("缺少捕获时长"))?,
            request
                .capacity_mi_b
                .ok_or_else(|| AppError::invalid_argument("缺少捕获容量"))?,
            Some(scope),
            now,
        ),
        "stop" => current.stop_expected(request.expected_session_id.as_deref(), now),
        _ => Err(AppError::invalid_argument("未知捕获操作")),
    }
}
pub(crate) fn set_capture(enabled: bool) -> CaptureStatus {
    let now = crate::services::common::now_unix_ms();
    let mut current = session();
    if enabled {
        if !CLEANING.load(Ordering::Acquire) && !current.view(now).detailed {
            let _ = current.start((CAPTURE_DURATION_MS / 1000) as u32, 10, None, now);
        }
    } else {
        current.stop("user", now);
    }
    current.view(now)
}
pub(super) struct CleanupGuard;
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        CLEANING.store(false, Ordering::Release);
    }
}
pub(super) fn begin_cleanup() -> AppResult<CleanupGuard> {
    CLEANING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| AppError::conflict("diagnostic_capture", "", "另一个捕获清理尚未结束"))?;
    stop_for_cleanup();
    Ok(CleanupGuard)
}
pub(crate) fn stop_for_cleanup() {
    let mut current = session();
    current.stop("cleanup", crate::services::common::now_unix_ms());
    current.storage_epoch = current.storage_epoch.saturating_add(1);
}
pub(super) fn storage_current(frame: &super::DebugFrame) -> bool {
    frame
        .payload
        .pointer("/_diagnosticCapture/storageEpoch")
        .and_then(Value::as_u64)
        .is_none_or(|epoch| epoch == session().storage_epoch)
}
pub(crate) fn capture_payload(value: &Value) -> Value {
    let completed = completed(value);
    let mut current = session();
    let status = current.view(crate::services::common::now_unix_ms());
    let epoch = current.storage_epoch;
    drop(current);
    let mut output = project(value, status.detailed, completed);
    output["_diagnosticCapture"]["storageEpoch"] = serde_json::json!(epoch);
    if status.detailed {
        output["_diagnosticCapture"]["sessionId"] = serde_json::json!(status.session_id);
    }
    output
}
fn completed(value: &Value) -> bool {
    ["event_type", "eventType", "type"]
        .iter()
        .filter_map(|key| value.get(key).and_then(Value::as_str))
        .any(|kind| matches!(kind, "flow.completed" | "connection.closed"))
}
pub(super) fn prepare_for_storage(
    mut frame: super::DebugFrame,
) -> (super::DebugFrame, Option<(String, u64)>) {
    let id = frame
        .payload
        .pointer("/_diagnosticCapture/sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let Some(id) = id else {
        return (frame, None);
    };
    let bytes = serde_json::to_vec(&frame).map_or(u64::MAX, |value| value.len() as u64 + 1);
    if session().admit(&id, bytes, crate::services::common::now_unix_ms()) {
        return (frame, Some((id, bytes)));
    }
    let storage_epoch = frame
        .payload
        .pointer("/_diagnosticCapture/storageEpoch")
        .cloned();
    frame.payload = project(&frame.payload, false, completed(&frame.payload));
    // A queued frame from an ended capture cannot retain a detailed marker.
    if let Some(meta) = frame.payload.get_mut("_diagnosticCapture") {
        *meta = serde_json::json!({"mode":"summary","truncated":true,"detailDiscarded":true,"storageEpoch":storage_epoch});
    }
    (frame, None)
}
pub(super) fn record_written(id: &str, bytes: u64, ok: bool) {
    session().written(id, bytes, ok, crate::services::common::now_unix_ms());
}

fn project(value: &Value, detailed: bool, completed: bool) -> Value {
    let mut budget = Budget {
        bytes: if completed {
            32 * 1024
        } else if detailed {
            16 * 1024
        } else {
            2 * 1024
        },
        nodes: if detailed || completed { 512 } else { 64 },
        depth: if detailed || completed { 12 } else { 3 },
        string: if detailed || completed { 1024 } else { 160 },
        truncated: false,
    };
    let mut result = budget.copy(value, 0);
    if let Some(object) = result.as_object_mut() {
        object.insert("_diagnosticCapture".into(), serde_json::json!({
            "mode": if detailed { "bounded-detail" } else if completed { "connection-history" } else { "summary" },
            "truncated": budget.truncated,
        }));
    }
    result
}

struct Budget {
    bytes: usize,
    nodes: usize,
    depth: usize,
    string: usize,
    truncated: bool,
}
impl Budget {
    fn copy(&mut self, value: &Value, depth: usize) -> Value {
        if self.nodes == 0 || self.bytes < 32 || depth > self.depth {
            self.truncated = true;
            return Value::String("[omitted]".into());
        }
        self.nodes -= 1;
        self.bytes = self.bytes.saturating_sub(32);
        match value {
            Value::Object(object) => {
                let mut output = Map::new();
                const ENVELOPE_KEYS: &[&str] = &[
                    "id",
                    "request_id",
                    "requestId",
                    "event_type",
                    "eventType",
                    "event",
                    "type",
                    "method",
                    "ok",
                ];
                // Envelope identity must survive a large payload regardless of JSON key order.
                let priority = ENVELOPE_KEYS
                    .iter()
                    .filter_map(|key| object.get_key_value(*key));
                let rest = object
                    .iter()
                    .filter(|(key, _)| !ENVELOPE_KEYS.contains(&key.as_str()));
                for (key, value) in priority.chain(rest) {
                    // Account for worst-case JSON escaping, object syntax and markers.
                    let key_cost = key.len().saturating_mul(6).saturating_add(32);
                    if self.nodes == 0 || key_cost >= self.bytes || key.len() > 128 {
                        self.truncated = true;
                        break;
                    }
                    self.bytes -= key_cost;
                    output.insert(
                        key.clone(),
                        if key == "config" {
                            Value::String("[configuration redacted]".into())
                        } else if crate::kernel::redaction::sensitive_key(key) {
                            Value::String("[redacted]".into())
                        } else {
                            self.copy(value, depth + 1)
                        },
                    );
                }
                Value::Object(output)
            }
            Value::Array(values) => {
                let mut output = Vec::new();
                for value in values.iter().take(32) {
                    if self.nodes == 0 || self.bytes < 32 {
                        break;
                    }
                    output.push(self.copy(value, depth + 1));
                }
                self.truncated |= output.len() < values.len();
                Value::Array(output)
            }
            Value::String(value) => {
                let max = self.string.min(self.bytes / 6);
                let mut end = value.len().min(max);
                while !value.is_char_boundary(end) {
                    end -= 1;
                }
                self.truncated |= end < value.len();
                self.bytes -= end * 6;
                Value::String(crate::kernel::redaction::text(&value[..end]))
            }
            value => value.clone(),
        }
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
