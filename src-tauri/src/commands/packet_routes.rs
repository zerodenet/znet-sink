use crate::{
    errors::{AppError, AppResult},
    models::{core::CoreIpcOptions, packet_routes::*},
    services::{common, core_config, packet_routes},
    state::app_state::AppState,
};
use tauri::State;
fn options(state: &AppState) -> AppResult<CoreIpcOptions> {
    Ok(core_config::ipc_options_from_app_config(
        &common::lock(state.app_config(), "app_config")?.core,
    ))
}
fn profile(state: &AppState) -> AppResult<Option<String>> {
    Ok(common::lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|p| p.active)
        .map(|p| p.id.clone()))
}
fn target_profile(state: &AppState, target: &PacketTarget) -> AppResult<()> {
    if profile(state)? != target.profile_id {
        return Err(AppError::conflict(
            "packet_route",
            &target.route_id,
            "当前配置已切换，请刷新 Packet 路径",
        ));
    }
    Ok(())
}
#[tauri::command]
pub async fn gui_packet_routes_snapshot(state: State<'_, AppState>) -> AppResult<PacketSnapshot> {
    let _guard = state.proxy_config_operation().lock().await;
    packet_routes::snapshot(profile(state.inner())?, options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_packet_route_detail(
    state: State<'_, AppState>,
    target: PacketTarget,
) -> AppResult<PacketRoute> {
    let _guard = state.proxy_config_operation().lock().await;
    target_profile(state.inner(), &target)?;
    packet_routes::detail(target, options(state.inner())?).await
}
#[tauri::command]
pub async fn gui_close_packet_route(
    state: State<'_, AppState>,
    target: PacketTarget,
) -> AppResult<PacketCloseResult> {
    let _guard = state.proxy_config_operation().lock().await;
    target_profile(state.inner(), &target)?;
    packet_routes::close(target, options(state.inner())?).await
}
