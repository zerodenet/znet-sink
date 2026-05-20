use serde_json::Value;
use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::models::core::{CoreCallResult, CoreEndpoint, CoreEventSubscription, CoreIpcOptions};
use crate::services::common::lock;
use crate::services::{control_plane, core_config, core_events, interaction_mode};
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
    control_plane::status(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_ping(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    control_plane::ping(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_query(
    state: State<'_, AppState>,
    request: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "rawIpc")?;
    control_plane::query(request, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_command(
    state: State<'_, AppState>,
    method: String,
    params: Option<Value>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "rawIpc")?;
    control_plane::command(method, params, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_ipc_request(
    state: State<'_, AppState>,
    frame: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "rawIpc")?;
    control_plane::request(frame, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_capabilities(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "diagnostics")?;
    control_plane::get_capabilities(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_health(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    control_plane::get_health(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_config(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    control_plane::get_config(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_runtime(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    control_plane::get_runtime(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_stats(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    control_plane::get_stats(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_get_policies(
    state: State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    control_plane::get_policies(resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_select_policy(
    state: State<'_, AppState>,
    policy_tag: String,
    target_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    control_plane::select_policy(policy_tag, target_tag, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_probe_policy(
    state: State<'_, AppState>,
    policy_tag: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "policyProbe")?;
    control_plane::probe_policy(policy_tag, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_close_flow(
    state: State<'_, AppState>,
    flow_id: String,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    control_plane::close_flow(flow_id, resolve_options(&state, options)?).await
}

#[tauri::command]
pub async fn core_validate_config(
    state: State<'_, AppState>,
    config: Value,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreCallResult> {
    interaction_mode::require_pro_mode(state.inner(), "coreConfig")?;
    control_plane::validate_config(config, resolve_options(&state, options)?).await
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
    state: &State<'_, AppState>,
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

fn default_options(state: &State<'_, AppState>) -> AppResult<CoreIpcOptions> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    Ok(control_plane::options_from_core_config(&config))
}
