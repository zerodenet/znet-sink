use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

use crate::errors::{AppError, AppResult};
use crate::models::core_process::CoreProcessState;
use crate::models::gui_core::{
    GuiConnection, GuiConnectionCloseResult, GuiConnectionList, GuiConnectionListOptions,
    GuiCoreHealth, GuiCoreOverview, GuiFeatureStatus, GuiPolicyGroup, GuiPolicySelectionResult,
    GuiTrafficSnapshot, GuiTrafficStats, GuiZeroCapabilities,
};
use crate::services::{core_process, interaction_mode, zero_adapter};
use crate::state::app_state::AppState;

const CORE_READY_WAIT_TIMEOUT: Duration = Duration::from_secs(8);
const CORE_READY_WAIT_INTERVAL: Duration = Duration::from_millis(100);

#[tauri::command]
pub async fn gui_core_overview(state: State<'_, AppState>) -> AppResult<GuiCoreOverview> {
    zero_adapter::core_overview(state.inner()).await
}

#[tauri::command]
pub async fn gui_core_health(state: State<'_, AppState>) -> AppResult<GuiCoreHealth> {
    zero_adapter::core_health(state.inner()).await
}

#[tauri::command]
pub async fn gui_zero_capabilities(state: State<'_, AppState>) -> AppResult<GuiZeroCapabilities> {
    zero_adapter::zero_capabilities(state.inner()).await
}

#[tauri::command]
pub async fn gui_traffic_stats(state: State<'_, AppState>) -> AppResult<GuiTrafficStats> {
    zero_adapter::traffic_stats(state.inner()).await
}

#[tauri::command]
pub async fn gui_traffic_snapshot(state: State<'_, AppState>) -> AppResult<GuiTrafficSnapshot> {
    zero_adapter::traffic_snapshot(state.inner()).await
}

#[tauri::command]
pub async fn gui_policy_groups(state: State<'_, AppState>) -> AppResult<Vec<GuiPolicyGroup>> {
    zero_adapter::policy_groups(state.inner()).await
}

#[tauri::command]
pub async fn gui_select_policy(
    state: State<'_, AppState>,
    policy_tag: String,
    target_tag: String,
) -> AppResult<GuiPolicySelectionResult> {
    zero_adapter::select_policy(state.inner(), policy_tag, target_tag).await
}

#[tauri::command]
pub async fn gui_connections(
    state: State<'_, AppState>,
    options: Option<GuiConnectionListOptions>,
) -> AppResult<GuiConnectionList> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    zero_adapter::connections(state.inner(), options).await
}

#[tauri::command]
pub async fn gui_connection_detail(
    state: State<'_, AppState>,
    flow_id: String,
) -> AppResult<GuiConnection> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    zero_adapter::connection_detail(state.inner(), flow_id).await
}

#[tauri::command]
pub async fn gui_close_connection(
    state: State<'_, AppState>,
    flow_id: String,
) -> AppResult<GuiConnectionCloseResult> {
    interaction_mode::require_pro_mode(state.inner(), "connections")?;
    zero_adapter::close_connection(state.inner(), flow_id).await
}

#[tauri::command]
pub async fn gui_dns_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    interaction_mode::require_pro_mode(state.inner(), "dns")?;
    zero_adapter::dns_status(state.inner()).await
}

#[tauri::command]
pub async fn gui_tun_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    zero_adapter::tun_status(state.inner()).await
}

#[tauri::command]
pub async fn gui_tun_enable(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<GuiFeatureStatus> {
    ensure_core_ready(app_handle, state.clone()).await?;
    zero_adapter::enable_tun(state.inner()).await
}

#[tauri::command]
pub async fn gui_tun_disable(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    zero_adapter::disable_tun(state.inner()).await
}

#[tauri::command]
pub async fn gui_stack_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    interaction_mode::require_pro_mode(state.inner(), "stack")?;
    zero_adapter::stack_status(state.inner()).await
}

#[tauri::command]
pub async fn gui_rule_status(state: State<'_, AppState>) -> AppResult<GuiFeatureStatus> {
    interaction_mode::require_pro_mode(state.inner(), "rules")?;
    zero_adapter::rule_status(state.inner()).await
}

async fn ensure_core_ready(app_handle: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    if zero_adapter::core_readiness_health(state.inner())
        .await
        .is_ok()
    {
        return Ok(());
    }

    let app_handle_start = app_handle.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle_start.state::<AppState>();
        core_process::start(app_handle_start.clone(), state)
    })
    .await
    .map_err(|error| AppError::internal(format!("core start thread panicked: {error}")))??;

    if status.state != CoreProcessState::Running {
        return Err(AppError::internal(
            "core process did not enter running state",
        ));
    }

    wait_for_core_ready(app_handle.state::<AppState>().inner()).await
}

async fn wait_for_core_ready(state: &AppState) -> AppResult<()> {
    let started = Instant::now();
    let mut last_error = None;

    while started.elapsed() < CORE_READY_WAIT_TIMEOUT {
        match zero_adapter::core_readiness_health(state).await {
            Ok(health) if health.healthy => return Ok(()),
            Ok(_) => return Err(AppError::internal("core readiness check reported unhealthy")),
            Err(error) => {
                last_error = Some(error);
                let _ = tauri::async_runtime::spawn_blocking(|| {
                    std::thread::sleep(CORE_READY_WAIT_INTERVAL);
                })
                .await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::internal("core readiness check timed out")))
}
