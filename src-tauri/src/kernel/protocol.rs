//! IPC protocol client for JSON-line frame communication.
//!
//! Provides the four frame types (`ping`, `query`, `command`, `subscribe`)
//! with automatic request-id assignment and response-id validation.
//!
//! This module is kernel-agnostic — it only knows about the JSON-line
//! protocol envelope. Kernel-specific query/command methods live in
//! their respective adapter modules (e.g. `kernel::zero`).

use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::config::{DEFAULT_IPC_TIMEOUT_MS, MAX_IPC_TIMEOUT_MS};
use crate::errors::{AppError, AppResult};
use crate::models::debug::{DebugFrame, DEBUG_RING_SIZE};

/// Static ring buffer for diagnostic IPC frame capture.
static DEBUG_FRAMES: std::sync::LazyLock<Mutex<Vec<DebugFrame>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::with_capacity(DEBUG_RING_SIZE)));

static DEBUG_FRAME_ID: AtomicU64 = AtomicU64::new(0);

/// Snapshot of all captured debug frames (newest first).
pub(crate) fn debug_frames_snapshot() -> Vec<DebugFrame> {
    DEBUG_FRAMES
        .lock()
        .map(|frames| {
            let mut v = frames.clone();
            v.reverse(); // newest first
            v
        })
        .unwrap_or_default()
}

/// Push a frame into the ring buffer, dropping oldest if at capacity.
fn debug_push(mut frame: DebugFrame) {
    frame.id = DEBUG_FRAME_ID.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut frames) = DEBUG_FRAMES.lock() {
        if frames.len() >= DEBUG_RING_SIZE {
            frames.remove(0);
        }
        frames.push(frame);
    }
}
use crate::kernel::transport;
use crate::models::core::{response_id, CoreCallResult, CoreEndpoint, CoreIpcOptions};

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

// ── Public API ──────────────────────────────────────────────────────

/// Resolve the default IPC endpoint from options or platform default.
pub fn endpoint_from_options(options: Option<&CoreIpcOptions>) -> AppResult<CoreEndpoint> {
    if let Some(socket) = options.and_then(|options| options.socket.as_deref()) {
        let socket = socket.trim();
        if !socket.is_empty() {
            return Ok(CoreEndpoint {
                transport: transport::transport_name(),
                path: socket.to_string(),
            });
        }
    }

    transport::default_endpoint("zero")
}

/// Resolve timeout from options, falling back to the application default.
pub fn timeout_from_options(options: Option<&CoreIpcOptions>) -> AppResult<Duration> {
    let timeout_ms = options
        .and_then(|options| options.timeout_ms)
        .unwrap_or(DEFAULT_IPC_TIMEOUT_MS);

    if timeout_ms == 0 || timeout_ms > MAX_IPC_TIMEOUT_MS {
        return Err(AppError {
            code: "invalid_argument",
            message: format!("timeoutMs must be between 1 and {MAX_IPC_TIMEOUT_MS}"),
            details: Some(json!({ "timeoutMs": timeout_ms })),
        });
    }

    Ok(Duration::from_millis(timeout_ms))
}

/// Send a bare `ping` frame.
pub async fn ping(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    request(json!({ "type": "ping" }), options).await
}

/// Send a `query` frame with the given request payload.
pub async fn query(
    query_request: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    request(
        json!({ "type": "query", "request": query_request }),
        options,
    )
    .await
}

/// Send a `command` frame with the given method and params.
pub async fn command(
    method: String,
    params: Option<Value>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    if method.trim().is_empty() {
        return Err(AppError::invalid_argument(
            "command method must not be empty",
        ));
    }

    let params = params.unwrap_or_else(|| Value::Object(Map::new()));
    if !params.is_object() {
        return Err(AppError::invalid_argument(
            "command params must be a JSON object",
        ));
    }

    request(
        json!({
            "type": "command",
            "method": method,
            "params": params,
        }),
        options,
    )
    .await
}

/// Send a raw IPC frame and return the response.
///
/// Uses the global multiplexed connection — all query, command, ping,
/// and subscribe frames share a SINGLE persistent pipe, matching the
/// kernel's recommended pattern and avoiding ERROR_PIPE_BUSY (231).
pub async fn request(
    frame: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    let endpoint = endpoint_from_options(options.as_ref())?;
    let timeout = timeout_from_options(options.as_ref())?;
    let (frame_value, request_id) = ensure_request_id(frame)?;
    let frame_type = frame_type_for_debug(&frame_value);
    let result_endpoint = endpoint.clone();

    // Capture outgoing frame
    debug_push(DebugFrame {
        id: 0,
        at_ms: crate::services::common::now_unix_ms(),
        direction: "tx",
        frame_type: frame_type.clone(),
        payload: frame_value.clone(),
        elapsed_ms: None,
        error: None,
    });

    let expected_id = request_id.clone();
    let (elapsed, response): (u64, Result<Value, AppError>) =
        tauri::async_runtime::spawn_blocking(move || {
            let t0 = std::time::Instant::now();
            let conn = super::connection::get_or_connect(endpoint, timeout)?;
            let response = conn.send_request(frame_value, timeout)?;
            validate_response_id(&response, expected_id.as_ref())?;
            let elapsed = t0.elapsed().as_millis() as u64;
            Ok((elapsed, Ok(response)))
        })
        .await
        .map_err(|error| AppError::internal(format!("IPC worker failed: {error}")))??;

    // Capture response frame
    match &response {
        Ok(value) => {
            debug_push(DebugFrame {
                id: 0,
                at_ms: crate::services::common::now_unix_ms(),
                direction: "rx",
                frame_type,
                payload: value.clone(),
                elapsed_ms: Some(elapsed),
                error: None,
            });
        }
        Err(error) => {
            debug_push(DebugFrame {
                id: 0,
                at_ms: crate::services::common::now_unix_ms(),
                direction: "rx",
                frame_type,
                payload: json!({}),
                elapsed_ms: Some(elapsed),
                error: Some(error.message.clone()),
            });
        }
    }

    Ok(CoreCallResult::from_core_result(
        result_endpoint,
        request_id,
        response,
    ))
}

/// Extract the frame type for debug capture.
fn frame_type_for_debug(frame: &Value) -> String {
    frame
        .as_object()
        .and_then(|obj| obj.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

// ── Convenience queries ─────────────────────────────────────────────

/// Query kernel capabilities.
pub async fn get_capabilities(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!({"capabilities": {}}), options).await
}

/// Query kernel health.
pub async fn get_health(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!({"health": {}}), options).await
}

/// Query kernel config snapshot.
pub async fn get_config(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!({"config": {}}), options).await
}

/// Query kernel runtime state.
pub async fn get_runtime(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!({"runtime": {}}), options).await
}

/// Query kernel traffic stats.
pub async fn get_stats(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!({"stats": {}}), options).await
}

/// Query all policy groups.
pub async fn get_policies(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!({"policies": {}}), options).await
}

/// Select a policy target.
pub async fn select_policy(
    policy_tag: String,
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    command(
        "policies.select".to_string(),
        Some(json!({
            "policy_tag": policy_tag,
            "target_tag": target_tag,
        })),
        options,
    )
    .await
}

/// Trigger a probe on a url_test policy group.
pub async fn probe_policy(
    policy_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    command(
        "policies.probe".to_string(),
        Some(json!({ "policy_tag": policy_tag })),
        options,
    )
    .await
}

/// Probe a single target's reachability.
pub async fn probe_target(
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    command(
        "diagnostics.probe_target".to_string(),
        Some(json!({ "target_tag": target_tag })),
        options,
    )
    .await
}

/// Close an active flow.
pub async fn close_flow(
    flow_id: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    command(
        "flows.close".to_string(),
        Some(json!({ "flow_id": flow_id })),
        options,
    )
    .await
}

/// Validate a config without applying it.
pub async fn validate_config(
    config: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    if !config.is_object() {
        return Err(AppError::invalid_argument("config must be a JSON object"));
    }

    command(
        "config.validate".to_string(),
        Some(json!({ "config": config })),
        options,
    )
    .await
}

// ── Internal helpers ────────────────────────────────────────────────

fn ensure_request_id(mut frame: Value) -> AppResult<(Value, Option<Value>)> {
    let object = frame.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("IPC frame must be a JSON object")
    })?;

    let id = object
        .get("id")
        .cloned()
        .unwrap_or_else(|| {
            let id = Value::String(next_request_id());
            object.insert("id".to_string(), id.clone());
            id
        });

    Ok((frame, Some(id)))
}

fn next_request_id() -> String {
    let sequence = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    format!("znet-sink-{sequence}")
}

fn validate_response_id(response: &Value, expected_id: Option<&Value>) -> AppResult<()> {
    let Some(expected_id) = expected_id else {
        return Ok(());
    };
    let actual_id = response_id(response).ok_or_else(|| AppError {
        code: "invalid_response",
        message: "core IPC response did not include request id".to_string(),
        details: Some(json!({
            "expectedId": expected_id,
            "response": response,
        })),
    })?;

    if &actual_id != expected_id {
        return Err(AppError {
            code: "invalid_response",
            message: "core IPC response id did not match request id".to_string(),
            details: Some(json!({
                "expectedId": expected_id,
                "actualId": actual_id,
                "response": response,
            })),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_request_id, validate_response_id};
    use serde_json::json;

    #[test]
    fn request_id_is_added_when_missing() {
        let (frame, request_id) = ensure_request_id(json!({ "type": "ping" })).unwrap();

        assert!(request_id.is_some());
        assert_eq!(frame.get("id"), request_id.as_ref());
    }

    #[test]
    fn caller_request_id_is_preserved() {
        let (frame, request_id) =
            ensure_request_id(json!({ "id": "caller-1", "type": "ping" })).unwrap();

        assert_eq!(request_id, Some(json!("caller-1")));
        assert_eq!(frame["id"], json!("caller-1"));
    }

    #[test]
    fn matching_response_id_is_accepted() {
        validate_response_id(&json!({ "id": "caller-1", "ok": true }), Some(&json!("caller-1")))
            .unwrap();
    }

    #[test]
    fn missing_response_id_is_rejected() {
        let error = validate_response_id(&json!({ "ok": true }), Some(&json!("caller-1")))
            .unwrap_err();

        assert_eq!(error.code, "invalid_response");
    }

    #[test]
    fn mismatched_response_id_is_rejected() {
        let error = validate_response_id(
            &json!({ "id": "other", "ok": true }),
            Some(&json!("caller-1")),
        )
        .unwrap_err();

        assert_eq!(error.code, "invalid_response");
    }
}
