use serde_json::{json, Map, Value};
use std::time::Duration;

use crate::config::{DEFAULT_IPC_TIMEOUT_MS, MAX_IPC_TIMEOUT_MS};
use crate::core::ipc;
use crate::errors::{AppError, AppResult};
use crate::models::{
    app_config::AppCoreConfig,
    core::{CoreCallResult, CoreEndpoint, CoreIpcOptions},
};
use crate::services::core_config;

pub fn default_endpoint() -> AppResult<CoreEndpoint> {
    ipc::default_endpoint()
}

pub async fn status(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    ping(options).await
}

pub async fn ping(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    request(json!({ "type": "ping" }), options).await
}

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

pub async fn request(frame: Value, options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    let endpoint = endpoint_from_options(options.as_ref())?;
    let timeout = timeout_from_options(options.as_ref())?;
    let frame = ipc::serialize_frame(&frame)?;
    let result_endpoint = endpoint.clone();

    let response = tauri::async_runtime::spawn_blocking(move || {
        ipc::send_json_line_request(endpoint, frame, timeout)
    })
    .await
    .map_err(|error| AppError::internal(format!("IPC worker failed: {error}")))?;

    Ok(CoreCallResult::from_core_result(result_endpoint, response))
}

pub async fn get_capabilities(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!("Capabilities"), options).await
}

pub async fn get_health(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!("Health"), options).await
}

pub async fn get_config(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!("Config"), options).await
}

pub async fn get_runtime(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!("Runtime"), options).await
}

pub async fn get_stats(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!("Stats"), options).await
}

pub async fn get_policies(options: Option<CoreIpcOptions>) -> AppResult<CoreCallResult> {
    query(json!("Policies"), options).await
}

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

pub fn endpoint_from_options(options: Option<&CoreIpcOptions>) -> AppResult<CoreEndpoint> {
    if let Some(socket) = options.and_then(|options| options.socket.as_deref()) {
        let socket = socket.trim();
        if !socket.is_empty() {
            return Ok(CoreEndpoint {
                transport: transport_name(),
                path: socket.to_string(),
            });
        }
    }

    default_endpoint()
}

pub fn options_from_core_config(config: &AppCoreConfig) -> CoreIpcOptions {
    core_config::ipc_options_from_app_config(config)
}

pub(crate) fn timeout_from_options(options: Option<&CoreIpcOptions>) -> AppResult<Duration> {
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

#[cfg(windows)]
fn transport_name() -> &'static str {
    "named-pipe"
}

#[cfg(unix)]
fn transport_name() -> &'static str {
    "unix-socket"
}
