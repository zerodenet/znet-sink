use crate::errors::AppResult;
use crate::models::core::CoreIpcOptions;
use crate::services::{
    common, core_config, interaction_mode,
    network_endpoint::{
        self,
        managed::{self, ManagedCatalog, ManagedInput},
    },
};
use crate::state::app_state::AppState;
use serde_json::Value;
use tauri::State;

fn options(state: &AppState) -> AppResult<CoreIpcOptions> {
    interaction_mode::require_pro_mode(state, "network_endpoints")?;
    Ok(core_config::ipc_options_from_app_config(
        &common::lock(state.app_config(), "app_config")?.core,
    ))
}

#[tauri::command]
pub async fn gui_endpoint_catalog(state: State<'_, AppState>) -> AppResult<ManagedCatalog> {
    let _operation = state.proxy_config_operation().lock().await;
    managed::catalog(state.inner(), options(state.inner())?).await
}

#[tauri::command]
pub async fn gui_endpoint_details(
    state: State<'_, AppState>,
    endpoint_id: String,
    core_instance_id: String,
) -> AppResult<Value> {
    network_endpoint::details(endpoint_id, core_instance_id, options(state.inner())?).await
}

#[tauri::command]
pub async fn gui_endpoint_control(
    state: State<'_, AppState>,
    input: ManagedInput,
) -> AppResult<Value> {
    let _operation = state.proxy_config_operation().lock().await;
    managed::control(state.inner(), input, options(state.inner())?).await
}
