//! Bounded, opt-in IPC detail capture. Never serialize or clone the original tree.
use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::atomic::{AtomicU64, Ordering};

const CAPTURE_DURATION_MS: u64 = 5 * 60 * 1_000;
static CAPTURE_UNTIL: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub detailed: bool,
    pub expires_at_ms: u64,
}

pub(crate) fn capture_status() -> CaptureStatus {
    status_at(crate::services::common::now_unix_ms())
}

fn status_at(now: u64) -> CaptureStatus {
    let expires_at_ms = CAPTURE_UNTIL.load(Ordering::Relaxed);
    CaptureStatus {
        detailed: expires_at_ms > now,
        expires_at_ms,
    }
}

pub(crate) fn set_capture(enabled: bool) -> CaptureStatus {
    let now = crate::services::common::now_unix_ms();
    CAPTURE_UNTIL.store(
        if enabled {
            now.saturating_add(CAPTURE_DURATION_MS)
        } else {
            0
        },
        Ordering::Relaxed,
    );
    status_at(now)
}

pub(crate) fn capture_payload(value: &Value) -> Value {
    let completed = ["event_type", "eventType", "type"]
        .iter()
        .filter_map(|key| value.get(key).and_then(Value::as_str))
        .any(|kind| matches!(kind, "flow.completed" | "connection.closed"));
    project(value, capture_status().detailed, completed)
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
