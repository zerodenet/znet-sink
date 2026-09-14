use serde_json::Value;
use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::kernel::protocol as ipc;
use crate::kernel::zero::queries;
use crate::models::core::{CoreCallResult, CoreEndpoint, CoreEventSubscription, CoreIpcOptions};
use crate::services::common::lock;
use crate::services::{core_config, core_events, interaction_mode};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn core_ipc_default_endpoint(state: State<'_, AppState>) -> AppResult<CoreEndpoint> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    Ok(core_config::snapshot_from_config(&config)?.endpoint)
}

#[tauri::command]
pub async fn core_status(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    ipc::ping(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_ping(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    ipc::ping(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_query(
    state: State<'_, AppState>,
    request: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "rawIpc")?;
    ipc::query(request, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_command(
    state: State<'_, AppState>,
    method: String,
    params: Option<Value>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "rawIpc")?;
    if matches!(method.as_str(), "config.apply" | "config.apply_runtime") {
        return managed_config_command(state.inner(), params.unwrap_or_default(), options, None)
            .await;
    }
    ipc::command(method, params, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_request(
    state: State<'_, AppState>,
    frame: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "rawIpc")?;
    if frame.get("type").and_then(Value::as_str) == Some("command")
        && matches!(
            frame.get("method").and_then(Value::as_str),
            Some("config.apply" | "config.apply_runtime")
        )
    {
        return managed_config_command(
            state.inner(),
            frame.get("params").cloned().unwrap_or_default(),
            options,
            frame.get("id").cloned(),
        )
        .await;
    }
    ipc::request(frame, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_capabilities(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    interaction_mode::require_pro_mode(state.inner(), "diagnostics")?;
    queries::query_value(
        serde_json::json!({"capabilities": {}}),
        "capabilities",
        resolve_options(&state, options)?,
    )
    .await
}

#[tauri::command]
pub async fn core_get_health(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    queries::query_value(
        serde_json::json!({"health": {}}),
        "health",
        resolve_options(&state, options)?,
    )
    .await
}

#[tauri::command]
pub async fn core_get_config(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    queries::query_value(
        serde_json::json!({"config": {}}),
        "config",
        resolve_options(&state, options)?,
    )
    .await
}

#[tauri::command]
pub async fn core_get_runtime(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    queries::query_value(
        serde_json::json!({"runtime": {}}),
        "runtime",
        resolve_options(&state, options)?,
    )
    .await
}

#[tauri::command]
pub async fn core_get_stats(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    queries::query_value(
        serde_json::json!({"stats": {}}),
        "stats",
        resolve_options(&state, options)?,
    )
    .await
}

#[tauri::command]
pub async fn core_get_policies(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    queries::query_value(
        serde_json::json!({"policies": {}}),
        "policies",
        resolve_options(&state, options)?,
    )
    .await
}

#[tauri::command]
pub async fn core_select_policy(
    state: State<'_, AppState>,
    policy_tag: String,
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    ipc::select_policy(policy_tag, target_tag, resolve_options(&state, options)?).await
}

#[cfg(feature = "tool-node-probe")]
#[tauri::command]
pub async fn core_probe_policy(
    state: State<'_, AppState>,
    policy_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "policyProbe")?;
    ipc::probe_policy(policy_tag, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_close_flow(
    state: State<'_, AppState>,
    flow_id: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    ipc::close_flow(flow_id, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_validate_config(
    state: State<'_, AppState>,
    config: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    ipc::validate_config(config, resolve_options(&state, options)?).await
}

#[tauri::command]
pub fn core_events_start(
    app: AppHandle,
    state: State<'_, AppState>,
    events: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreEventSubscription> {
    let generation = state.next_core_event_generation();
    let options = resolve_options(&state, options)?;
    core_events::start(
        app,
        state.core_event_generation(),
        generation,
        events,
        options,
    )
}

#[tauri::command]
pub fn core_events_stop(state: State<'_, AppState>) -> u64 {
    state.next_core_event_generation()
}

fn resolve_options(
    state: &AppState,
    options: Option<CoreIpcOptions>,
) -> AppResult<Option<CoreIpcOptions>> {
    if options
        .as_ref()
        .and_then(|options| options.socket.as_deref())
        .is_some_and(|socket| !socket.trim().is_empty())
    {
        return Ok(options);
    }

    let mut resolved = default_options(state)?;
    if let Some(timeout_ms) = options.and_then(|options| options.timeout_ms) {
        resolved.timeout_ms = Some(timeout_ms);
    }
    Ok(Some(resolved))
}

fn default_options(state: &AppState) -> AppResult<CoreIpcOptions> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    Ok(core_config::ipc_options_from_app_config(&config))
}

async fn managed_config_command(
    state: &AppState,
    params: Value,
    options: Option<CoreIpcOptions>,
    id: Option<Value>,
) -> AppResult<CoreCallResult> {
    let _operation = state.proxy_config_operation().lock().await;
    let config = params
        .get("config")
        .cloned()
        .ok_or_else(|| crate::errors::AppError::invalid_argument("config is required"))?;
    let options = resolve_options(state, options)?.unwrap_or_default();
    let endpoint = ipc::endpoint_from_options(Some(&options))?;
    let result = crate::configuration::apply::apply_with_identity(state.capabilities(), config, options).await.map(|identity| serde_json::json!({
        "api_id":"zero.api.v1", "id":id, "ok":true,
        "result":{"accepted":true,"result":{"applied":true,"persistence":"runtime_only","core_instance_id":identity.core_instance_id,"config_revision":identity.config_revision}}
    }));
    Ok(CoreCallResult::from_core_result(endpoint, id, result))
}
