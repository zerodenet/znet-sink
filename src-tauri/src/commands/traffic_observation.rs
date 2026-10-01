use crate::{
    errors::AppResult,
    models::core::CoreIpcOptions,
    services::{
        common, core_config, interaction_mode,
        traffic_observation::{self, ListInput, ResetInput, Scope},
    },
    state::app_state::AppState,
};
use serde_json::Value;
use tauri::State;
fn options(state: &AppState) -> AppResult<CoreIpcOptions> {
    interaction_mode::require_pro_mode(state, "traffic_observation")?;
    Ok(core_config::ipc_options_from_app_config(
        &common::lock(state.app_config(), "app_config")?.core,
    ))
}
#[tauri::command]
pub async fn gui_traffic_discover(state: State<'_, AppState>) -> AppResult<Value> {
    traffic_observation::discover(options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_traffic_page(state: State<'_, AppState>, input: ListInput) -> AppResult<Value> {
    traffic_observation::page(input, options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_traffic_get(state: State<'_, AppState>, scope: Scope) -> AppResult<Value> {
    traffic_observation::get(scope, options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_traffic_reset(state: State<'_, AppState>, input: ResetInput) -> AppResult<Value> {
    let _operation = state.proxy_config_operation().lock().await;
    traffic_observation::reset(input, options(state.inner())?).await
}
