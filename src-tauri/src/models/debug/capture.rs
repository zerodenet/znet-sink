//! Bounded, opt-in IPC detail capture with a metadata-only recovery journal.
use crate::{
    client_core::ClientScope,
    errors::{AppError, AppResult},
    services::capture_recovery::{
        CaptureConfirmation, CaptureEndReason, CapturePhase, CaptureRecord, CaptureRecoveryStore,
        RECORD_LIMIT,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        LazyLock, Mutex,
    },
};
const CAPTURE_DURATION_MS: u64 = 300_000;
const MIB: u64 = 1024 * 1024;
static CLEANING: AtomicBool = AtomicBool::new(false);
static QUEUE_LOSSES: AtomicU64 = AtomicU64::new(0);
static SESSION: LazyLock<Mutex<Session>> = LazyLock::new(|| {
    Mutex::new(Session {
        storage_required: true,
        ..Session::default()
    })
});
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
    pub recovery_state: &'static str,
    pub recovery_reason: Option<&'static str>,
    pub recovered: bool,
    pub recovery_history: Vec<CaptureRecord>,
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
    recovery: Option<CaptureRecoveryStore>,
    records: Vec<CaptureRecord>,
    storage_required: bool,
    recovery_failed: bool,
    recovered: bool,
    checkpoint_sequence: u64,
}
struct Checkpoint {
    sequence: u64,
    records: Vec<CaptureRecord>,
    store: CaptureRecoveryStore,
}
fn recovery_error() -> AppError {
    AppError { code:"capture_recovery_unconfirmed",message:"捕获恢复记录存盘或读取未确认；详细捕获已停止或未启动，不会自动重放。请核对或显式清理捕获恢复记录".into(),details:None }
}
fn end_reason(reason: Option<&str>) -> Option<CaptureEndReason> {
    match reason {
        Some("duration") => Some(CaptureEndReason::Duration),
        Some("capacity") => Some(CaptureEndReason::Capacity),
        Some("user") => Some(CaptureEndReason::User),
        Some("cleanup") => Some(CaptureEndReason::Cleanup),
        Some("storage_failure") => Some(CaptureEndReason::StorageFailure),
        Some("scope_changed") => Some(CaptureEndReason::ScopeChanged),
        Some("process_restart") => Some(CaptureEndReason::ProcessRestart),
        Some("journal_failure") => Some(CaptureEndReason::JournalFailure),
        _ => None,
    }
}
fn reason_name(reason: Option<CaptureEndReason>) -> Option<&'static str> {
    match reason {
        Some(CaptureEndReason::Duration) => Some("duration"),
        Some(CaptureEndReason::Capacity) => Some("capacity"),
        Some(CaptureEndReason::User) => Some("user"),
        Some(CaptureEndReason::Cleanup) => Some("cleanup"),
        Some(CaptureEndReason::StorageFailure) => Some("storage_failure"),
        Some(CaptureEndReason::ScopeChanged) => Some("scope_changed"),
        Some(CaptureEndReason::ProcessRestart) => Some("process_restart"),
        Some(CaptureEndReason::JournalFailure) => Some("journal_failure"),
        None => None,
    }
}
impl Session {
    fn idle() -> CaptureStatus {
        CaptureStatus {
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
            recovery_state: "unavailable",
            recovery_reason: None,
            recovered: false,
            recovery_history: Vec::new(),
        }
    }
    fn record(status: &CaptureStatus) -> Option<CaptureRecord> {
        let phase = match status.state {
            "capturing" => CapturePhase::Capturing,
            "stopped" => CapturePhase::Stopped,
            _ => CapturePhase::Interrupted,
        };
        Some(CaptureRecord {
            session_id: status.session_id.clone()?,
            scope: status.scope.clone(),
            phase,
            confirmation: if phase == CapturePhase::Interrupted {
                CaptureConfirmation::ResultUnknown
            } else {
                CaptureConfirmation::Confirmed
            },
            started_at_ms: status.started_at_ms,
            expires_at_ms: status.expires_at_ms,
            ended_at_ms: status.ended_at_ms,
            duration_seconds: status.duration_seconds,
            maximum_bytes: status.maximum_bytes,
            stored_bytes: status.stored_bytes,
            stored_frames: status.stored_frames,
            queue_drops_observed: status.queue_drops_observed,
            end_reason: end_reason(status.end_reason),
        })
    }
    fn from_record(record: &CaptureRecord) -> CaptureStatus {
        CaptureStatus {
            detailed: false,
            expires_at_ms: record.expires_at_ms,
            session_id: Some(record.session_id.clone()),
            started_at_ms: record.started_at_ms,
            ended_at_ms: record.ended_at_ms,
            duration_seconds: record.duration_seconds,
            maximum_bytes: record.maximum_bytes,
            stored_bytes: record.stored_bytes,
            stored_frames: record.stored_frames,
            queue_drops_observed: record.queue_drops_observed,
            state: if record.phase == CapturePhase::Stopped {
                "stopped"
            } else {
                "interrupted"
            },
            end_reason: reason_name(record.end_reason),
            scope: record.scope.clone(),
            ..Self::idle()
        }
    }
    fn initialize(&mut self, path: &Path) -> AppResult<()> {
        if self.recovery.is_some() {
            return Err(AppError::conflict(
                "diagnostic_capture",
                "",
                "捕获恢复账本已初始化，不允许重读覆盖本进程状态",
            ));
        }
        self.storage_required = true;
        let store = CaptureRecoveryStore::new(path);
        self.recovery = Some(store.clone());
        self.recovery_failed = true;
        let mut records = store.load()?;
        let mut changed = false;
        for record in &mut records {
            if record.phase == CapturePhase::Capturing {
                record.phase = CapturePhase::Interrupted;
                record.confirmation = CaptureConfirmation::ResultUnknown;
                record.end_reason = Some(CaptureEndReason::ProcessRestart);
                record.ended_at_ms = None;
                changed = true;
            }
        }
        // Publish conservative recovered facts even when this projection cannot
        // be checkpointed. A later start is blocked; no old action is replayed.
        self.records = records;
        self.status = self.records.last().map(Self::from_record);
        self.recovered = self.status.is_some();
        self.deadline = None;
        if changed {
            store.save(&self.records)?;
        }
        self.recovery_failed = false;
        Ok(())
    }
    fn prepare_checkpoint(&mut self) -> AppResult<Option<Checkpoint>> {
        if !self.storage_required {
            return Ok(None);
        }
        if self.recovery_failed {
            return Err(recovery_error());
        }
        let Some(store) = self.recovery.clone() else {
            return self.persistence_failed();
        };
        let Some(record) = self.status.as_ref().and_then(Self::record) else {
            return Ok(None);
        };
        let mut next = self.records.clone();
        if let Some(existing) = next
            .iter_mut()
            .find(|existing| existing.session_id == record.session_id)
        {
            *existing = record;
        } else {
            next.push(record);
            if next.len() > RECORD_LIMIT {
                next.remove(0);
            }
        }
        if next == self.records {
            return Ok(None);
        }
        let Some(sequence) = self.checkpoint_sequence.checked_add(1) else {
            return self.persistence_failed();
        };
        self.checkpoint_sequence = sequence;
        Ok(Some(Checkpoint {
            sequence: self.checkpoint_sequence,
            records: next,
            store,
        }))
    }
    fn complete_checkpoint(
        &mut self,
        checkpoint: Checkpoint,
        result: AppResult<bool>,
    ) -> AppResult<()> {
        if checkpoint.sequence != self.checkpoint_sequence {
            return Ok(());
        }
        match result {
            Ok(true) => {
                self.records = checkpoint.records;
                Ok(())
            }
            Ok(false) => Ok(()),
            Err(_) => self.persistence_failed(),
        }
    }
    fn checkpoint(&mut self) -> AppResult<()> {
        let Some(checkpoint) = self.prepare_checkpoint()? else {
            return Ok(());
        };
        let result = checkpoint
            .store
            .save_ordered(checkpoint.sequence, &checkpoint.records);
        self.complete_checkpoint(checkpoint, result)
    }
    fn persistence_failed<T>(&mut self) -> AppResult<T> {
        self.recovery_failed = true;
        self.deadline = None;
        if let Some(status) = &mut self.status {
            status.detailed = false;
            status.state = "interrupted";
            status.end_reason = Some("journal_failure");
            status.ended_at_ms = None;
        }
        Err(recovery_error())
    }
    fn expire(&mut self, now: u64) {
        let mut changed = false;
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
                changed = true;
            }
        }
        // Healthy producer reads never write counter checkpoints. Only a
        // terminal transition needs publication; counters checkpoint in worker.
        if changed {
            let _ = self.checkpoint();
        }
    }
    fn view(&mut self, now: u64) -> CaptureStatus {
        self.expire(now);
        let mut status = self.status.clone().unwrap_or_else(Self::idle);
        status.recovery_state = if self.recovery_failed {
            "result_unknown"
        } else if self.recovery.is_some() {
            "confirmed"
        } else {
            "unavailable"
        };
        status.recovery_reason = if status.recovery_state == "confirmed" {
            None
        } else {
            Some("捕获恢复账本不可用或尚未确认；不会恢复或重放旧捕获")
        };
        status.recovery_history = self.records.clone();
        status.recovered = self.recovered;
        status
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
        let was_detailed = self.view(now).detailed;
        if self.storage_required && (self.recovery_failed || self.recovery.is_none()) {
            return Err(recovery_error());
        }
        if was_detailed {
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
        self.recovered = false;
        self.status = Some(CaptureStatus {
            detailed: true,
            expires_at_ms: now.saturating_add(u64::from(duration) * 1000),
            session_id: Some(format!("capture-{id}")),
            started_at_ms: now,
            duration_seconds: duration,
            maximum_bytes: u64::from(capacity) * MIB,
            state: "capturing",
            scope,
            ..Self::idle()
        });
        self.checkpoint()?;
        Ok(self.view(now))
    }
    fn stop(&mut self, reason: &'static str, now: u64) -> AppResult<()> {
        self.expire(now);
        if self.recovery_failed {
            return Err(recovery_error());
        }
        if let Some(status) = &mut self.status {
            if status.detailed {
                status.detailed = false;
                status.state = "stopped";
                status.end_reason = Some(reason);
                status.ended_at_ms = Some(now);
            }
        }
        self.checkpoint()
    }
    fn invalidate(&mut self, scope: &ClientScope, now: u64) {
        if self
            .status
            .as_ref()
            .and_then(|status| status.scope.as_ref())
            .is_some_and(|original| original != scope)
        {
            let _ = self.stop("scope_changed", now);
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
        // Interrupted historical state cannot become a confirmed user stop.
        if self
            .status
            .as_ref()
            .is_some_and(|status| status.state == "interrupted")
        {
            return Err(recovery_error());
        }
        self.stop("user", now)?;
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
            let _ = self.stop("capacity", now);
            return false;
        }
        true
    }
    fn written_deferred(
        &mut self,
        id: &str,
        bytes: u64,
        ok: bool,
        now: u64,
    ) -> AppResult<Option<Checkpoint>> {
        let Some(status) = &mut self.status else {
            return Ok(None);
        };
        if status.session_id.as_deref() != Some(id) {
            return Ok(None);
        }
        if ok {
            status.stored_bytes = status.stored_bytes.saturating_add(bytes);
            status.stored_frames = status.stored_frames.saturating_add(1);
        }
        if status.detailed && (!ok || status.stored_bytes >= status.maximum_bytes) {
            status.detailed = false;
            status.state = "stopped";
            status.end_reason = Some(if ok { "capacity" } else { "storage_failure" });
            status.ended_at_ms = Some(now);
        }
        self.prepare_checkpoint()
    }
    #[cfg(test)]
    fn written(&mut self, id: &str, bytes: u64, ok: bool, now: u64) {
        if let Ok(Some(checkpoint)) = self.written_deferred(id, bytes, ok, now) {
            let result = checkpoint
                .store
                .save_ordered(checkpoint.sequence, &checkpoint.records);
            let _ = self.complete_checkpoint(checkpoint, result);
        }
    }
    fn clear_recovery(&mut self) -> AppResult<()> {
        if self.status.as_ref().is_some_and(|status| status.detailed) {
            return Err(AppError::conflict(
                "diagnostic_capture",
                "",
                "请先协调捕获写入再清理恢复记录",
            ));
        }
        let Some(store) = &self.recovery else {
            return Err(recovery_error());
        };
        if store.clear().is_err() {
            return self.persistence_failed();
        }
        self.records.clear();
        self.status = None;
        self.deadline = None;
        self.recovered = false;
        self.recovery_failed = false;
        Ok(())
    }
}
fn session() -> std::sync::MutexGuard<'static, Session> {
    SESSION.lock().unwrap_or_else(|error| error.into_inner())
}
pub(crate) fn initialize_capture_recovery(path: &Path) -> AppResult<()> {
    session().initialize(path)
}
/// Only the capture cleanup coordinator may call this after its writer barrier.
pub(crate) fn clear_capture_recovery() -> AppResult<()> {
    if !CLEANING.load(Ordering::Acquire) {
        return Err(AppError::conflict(
            "diagnostic_capture",
            "",
            "清理恢复记录须先协调捕获写入",
        ));
    }
    session().clear_recovery()
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
pub(crate) fn set_capture(enabled: bool) -> AppResult<CaptureStatus> {
    let now = crate::services::common::now_unix_ms();
    let mut current = session();
    if enabled {
        if CLEANING.load(Ordering::Acquire) {
            return Err(AppError::conflict(
                "diagnostic_capture",
                "",
                "正在协调捕获清理，请稍后再开始",
            ));
        }
        if !current.view(now).detailed {
            current.start((CAPTURE_DURATION_MS / 1000) as u32, 10, None, now)?;
        }
    } else {
        current.stop("user", now)?;
    }
    Ok(current.view(now))
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
    // An unavailable journal cannot prevent users from explicitly clearing its
    // corrupt metadata. Preserve the uncertainty in status until clear succeeds.
    stop_for_cleanup();
    Ok(CleanupGuard)
}
pub(crate) fn stop_for_cleanup() {
    let mut current = session();
    let _ = current.stop("cleanup", crate::services::common::now_unix_ms());
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
    current.expire(crate::services::common::now_unix_ms());
    let detailed = current
        .status
        .as_ref()
        .is_some_and(|status| status.detailed);
    let id = current
        .status
        .as_ref()
        .and_then(|status| status.session_id.clone());
    let epoch = current.storage_epoch;
    drop(current);
    let mut output = project(value, detailed, completed);
    output["_diagnosticCapture"]["storageEpoch"] = serde_json::json!(epoch);
    if detailed {
        output["_diagnosticCapture"]["sessionId"] = serde_json::json!(id);
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
    if let Some(meta) = frame.payload.get_mut("_diagnosticCapture") {
        *meta = serde_json::json!({"mode":"summary","truncated":true,"detailDiscarded":true,"storageEpoch":storage_epoch});
    }
    (frame, None)
}
pub(super) fn record_written(id: &str, bytes: u64, ok: bool) {
    let checkpoint = {
        let mut current = session();
        current
            .written_deferred(id, bytes, ok, crate::services::common::now_unix_ms())
            .ok()
            .flatten()
    };
    if let Some(checkpoint) = checkpoint {
        // Disk I/O runs on the existing debug worker after releasing capture;
        // healthy transport producers never wait on a counter fsync.
        let result = checkpoint
            .store
            .save_ordered(checkpoint.sequence, &checkpoint.records);
        let _ = session().complete_checkpoint(checkpoint, result);
    }
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
