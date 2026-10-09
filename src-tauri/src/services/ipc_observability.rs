//! Compact kernel-control summaries in the running log. Polling responses and
//! full JSON belong only in the bounded IPC store, never in a second log cache.

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
            if requests.len() >= 256 {
                requests.clear();
            }
            requests.insert(request_id.clone(), request.clone());
        }
        append_frame_log(
            state,
            LogSource::App,
            frame,
            &request,
            Some(&request_id),
            format!("内核 IPC 请求（{}）", request.label),
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
                format!("内核 IPC 事件（{}）", request.label),
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
                format!("内核 IPC 请求失败（{}）", request.label),
                "client_transport_error",
            );
        } else {
            append_frame_log(
                state,
                LogSource::Core,
                frame,
                &request,
                Some(&request_id),
                format!("内核 IPC 响应（{}）", request.label),
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
        // Background policy polling is available in IPC diagnostics, not duplicated
        // into the normal running log on every refresh.
        "query" => None,
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
    ["event_type", "eventType", "event", "type", "name", "method"]
        .iter()
        .filter_map(|key| payload.get(key).and_then(Value::as_str))
        .any(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("probe") || name.contains("url_test") || name.contains("urltest")
        })
}

fn event_label(payload: &Value) -> String {
    for key in ["event_type", "eventType", "event", "type", "name", "method"] {
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
            "summary": { "ok": frame.payload.get("ok"), "event": event_label(&frame.payload) },
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
        .is_none());
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
