//! Projection of selected raw kernel IPC frames into the normal log stream.
//!
//! The protocol/connection layers already capture exact JSON frames in the
//! debug store. This service observes that existing stream and mirrors only
//! node/diagnostic-relevant traffic into logs, preserving source semantics:
//! requests emitted by the GUI are `app`; successful responses and relevant
//! events received from Zero are `core`. Client-generated transport errors are
//! never mislabeled as kernel output.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

use crate::models::debug::DebugFrame;
use crate::models::logs::{LogLevel, LogSource};
use crate::services::logs;
use crate::state::app_state::AppState;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackedRequest {
    label: String,
    method: Option<String>,
}

#[derive(Default)]
pub struct IpcLogObserver {
    requests: Mutex<HashMap<String, TrackedRequest>>,
}

impl IpcLogObserver {
    pub fn observe(&self, state: &AppState, frame: &DebugFrame) {
        match frame.direction.as_str() {
            "tx" => self.observe_request(state, frame),
            "rx" => self.observe_incoming(state, frame),
            _ => {}
        }
    }

    fn observe_request(&self, state: &AppState, frame: &DebugFrame) {
        let Some(request) = classify_request(&frame.payload) else {
            return;
        };
        let Some(request_id) = frame_request_id(&frame.payload) else {
            return;
        };
        if let Ok(mut requests) = self.requests.lock() {
            requests.insert(request_id.clone(), request.clone());
        }
        append_frame_log(
            state,
            LogSource::App,
            frame,
            &request,
            Some(&request_id),
            format!("内核 IPC 原始请求（{}）", request.label),
            "app_request",
        );
    }

    fn observe_incoming(&self, state: &AppState, frame: &DebugFrame) {
        if frame.frame_type == "event" {
            if !is_relevant_event(&frame.payload) {
                return;
            }
            let request = TrackedRequest {
                label: event_label(&frame.payload),
                method: None,
            };
            append_frame_log(
                state,
                LogSource::Core,
                frame,
                &request,
                frame_request_id(&frame.payload).as_deref(),
                format!("内核 IPC 原始事件（{}）", request.label),
                "core_event",
            );
            return;
        }

        // Ignore the connection layer's multiplex/response summaries. The
        // protocol layer emits the complete raw response with frame_type equal
        // to the original command/query classification.
        if frame.frame_type != "command" && frame.frame_type != "query" {
            return;
        }
        let Some(request_id) = frame_request_id(&frame.payload) else {
            return;
        };
        let request = self
            .requests
            .lock()
            .ok()
            .and_then(|mut requests| requests.remove(&request_id));
        let Some(request) = request else {
            return;
        };

        if frame.error.is_some() {
            append_frame_log(
                state,
                LogSource::App,
                frame,
                &request,
                Some(&request_id),
                format!("内核 IPC 请求未收到原始响应（{}）", request.label),
                "client_transport_error",
            );
        } else {
            append_frame_log(
                state,
                LogSource::Core,
                frame,
                &request,
                Some(&request_id),
                format!("内核 IPC 原始响应（{}）", request.label),
                "core_response",
            );
        }
    }
}

fn classify_request(payload: &Value) -> Option<TrackedRequest> {
    let frame_type = payload.get("type")?.as_str()?;
    match frame_type {
        "command" => {
            let method = payload.get("method")?.as_str()?.trim();
            let relevant = method.starts_with("diagnostics.")
                || matches!(method, "policies.probe" | "policies.select");
            relevant.then(|| TrackedRequest {
                label: method.to_string(),
                method: Some(method.to_string()),
            })
        }
        "query" => payload
            .get("request")
            .and_then(Value::as_object)
            .is_some_and(|request| request.contains_key("policies"))
            .then(|| TrackedRequest {
                label: "query.policies".to_string(),
                method: None,
            }),
        _ => None,
    }
}

fn frame_request_id(payload: &Value) -> Option<String> {
    payload
        .get("id")
        .or_else(|| payload.get("request_id"))
        .or_else(|| payload.get("requestId"))
        .and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
}

fn is_relevant_event(payload: &Value) -> bool {
    serde_json::to_string(payload)
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("probe") || value.contains("url_test") || value.contains("urltest")
        })
        .unwrap_or(false)
}

fn event_label(payload: &Value) -> String {
    for key in ["event", "type", "name", "method"] {
        if let Some(value) = payload.get(key).and_then(Value::as_str) {
            if !value.trim().is_empty() {
                return value.to_string();
            }
        }
    }
    "probe-event".to_string()
}

fn append_frame_log(
    state: &AppState,
    source: LogSource,
    frame: &DebugFrame,
    request: &TrackedRequest,
    request_id: Option<&str>,
    message: String,
    origin: &str,
) {
    let _ = logs::append_entry(
        state,
        source,
        LogLevel::Debug,
        message,
        Some(serde_json::json!({
            "schema": "znet.kernel-ipc.v1",
            "area": "kernel",
            "operation": "ipc.frame",
            "origin": origin,
            "direction": frame.direction,
            "frameType": frame.frame_type,
            "frameId": frame.id,
            "capturedAtUnixMs": frame.at_ms,
            "requestId": request_id,
            "method": request.method,
            "elapsedMs": frame.elapsed_ms,
            "error": frame.error,
            "rawFrame": frame.payload,
        })),
    );
}

#[cfg(test)]
mod tests {
    use super::{classify_request, frame_request_id, is_relevant_event};
    use serde_json::json;

    #[test]
    fn tracks_probe_and_policy_interactions_but_not_unrelated_commands() {
        assert_eq!(
            classify_request(&json!({
                "id": "a",
                "type": "command",
                "method": "diagnostics.probe_outbound",
                "params": { "target_tag": "HK" }
            }))
            .unwrap()
            .label,
            "diagnostics.probe_outbound"
        );
        assert!(classify_request(&json!({
            "id": "b",
            "type": "command",
            "method": "config.apply",
            "params": {}
        }))
        .is_none());
        assert!(classify_request(&json!({
            "id": "c",
            "type": "query",
            "request": { "policies": {} }
        }))
        .is_some());
    }

    #[test]
    fn correlates_string_and_numeric_request_ids() {
        assert_eq!(
            frame_request_id(&json!({ "id": "abc" })).as_deref(),
            Some("abc")
        );
        assert_eq!(
            frame_request_id(&json!({ "requestId": 42 })).as_deref(),
            Some("42")
        );
    }

    #[test]
    fn limits_event_mirroring_to_probe_related_payloads() {
        assert!(is_relevant_event(
            &json!({ "event": "policy.probeCompleted" })
        ));
        assert!(is_relevant_event(&json!({ "type": "url_test.completed" })));
        assert!(!is_relevant_event(&json!({ "event": "traffic.updated" })));
    }
}
