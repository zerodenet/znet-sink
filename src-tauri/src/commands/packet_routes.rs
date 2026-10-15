use crate::{
    errors::{AppError, AppResult},
    models::{core::CoreIpcOptions, packet_routes::*},
    services::{common, core_config, packet_routes},
    state::app_state::AppState,
};
use tauri::State;
const JS_SAFE: u64 = 9_007_199_254_740_991;
fn valid_generation(value: u64) -> AppResult<()> {
    if value == 0 || value > JS_SAFE {
        return Err(AppError::invalid_argument(
            "Packet 客户端代次须为正安全整数，请更新客户端后重新检查",
        ));
    }
    Ok(())
}
fn ensure_host_session(current: &str, expected: &str) -> AppResult<()> {
    if current.is_empty() || expected.is_empty() || expected != current {
        return Err(AppError::conflict(
            "packet_host_session",
            "",
            "宿主进程已变化，请显式返回第一页重新发现；旧记录不恢复控制权限",
        ));
    }
    Ok(())
}
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
pub async fn gui_packet_routes_snapshot(
    state: State<'_, AppState>,
    query: Option<PacketPageQuery>,
) -> AppResult<PacketSnapshot> {
    let _guard = state.proxy_config_operation().lock().await;
    let host_session = state.host_session_id()?;
    let original = state.client_core_snapshot();
    let generation = original.scope_generation.0;
    valid_generation(generation)?;
    let query = query.unwrap_or_default();
    if let Some(expected) = &query.expected {
        ensure_host_session(host_session, &expected.host_session_id)?;
        valid_generation(expected.client_scope_generation)?;
        state.ensure_client_scope_generation(&original.scope, expected.client_scope_generation)?;
    }
    let mut result = packet_routes::snapshot(
        profile(state.inner())?,
        options(state.inner())?,
        query,
        generation,
    )
    .await?;
    state.ensure_client_scope_generation(&original.scope, generation)?;
    result.host_session_id = host_session.to_owned();
    Ok(result)
}
#[tauri::command]
pub async fn gui_packet_route_detail(
    state: State<'_, AppState>,
    target: PacketTarget,
) -> AppResult<PacketRoute> {
    let _guard = state.proxy_config_operation().lock().await;
    ensure_host_session(state.host_session_id()?, &target.host_session_id)?;
    valid_generation(target.client_scope_generation)?;
    let original = state.client_core_snapshot();
    state.ensure_client_scope_generation(&original.scope, target.client_scope_generation)?;
    target_profile(state.inner(), &target)?;
    let result = packet_routes::detail(target, options(state.inner())?).await;
    state.ensure_client_scope_generation(&original.scope, original.scope_generation.0)?;
    result
}
#[tauri::command]
pub async fn gui_close_packet_route(
    state: State<'_, AppState>,
    target: PacketTarget,
) -> AppResult<PacketCloseResult> {
    let _guard = state.proxy_config_operation().lock().await;
    ensure_host_session(state.host_session_id()?, &target.host_session_id)?;
    valid_generation(target.client_scope_generation)?;
    let original = state.client_core_snapshot();
    state.ensure_client_scope_generation(&original.scope, target.client_scope_generation)?;
    target_profile(state.inner(), &target)?;
    let result = packet_routes::close(target, options(state.inner())?).await;
    match result {
        Ok(result)
            if state
                .ensure_client_scope_generation(&original.scope, original.scope_generation.0)
                .is_err() =>
        {
            Ok(PacketCloseResult {
                operation_id: result.operation_id,
                status: "unconfirmed".into(),
                message: "客户端配置或实例代次已变化，原关闭仅供历史核对，不会重放".into(),
                route: None,
            })
        }
        other => other,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_generation_is_positive_and_never_rounded() {
        for invalid in [0, JS_SAFE + 1, u64::MAX] {
            assert!(valid_generation(invalid).is_err());
        }
        for valid in [1, JS_SAFE] {
            assert!(valid_generation(valid).is_ok());
        }
    }
}

#[cfg(test)]
mod host_recovery_tests {
    use super::*;
    #[test]
    fn a_new_host_with_equal_scope_generation_never_accepts_old_page_or_control_owner() {
        let first = AppState::default();
        let second = AppState::default();
        assert_eq!(
            first.client_core_snapshot().scope_generation,
            second.client_core_snapshot().scope_generation
        );
        assert!(ensure_host_session(
            first.host_session_id().unwrap(),
            first.host_session_id().unwrap()
        )
        .is_ok());
        assert!(ensure_host_session(
            second.host_session_id().unwrap(),
            first.host_session_id().unwrap()
        )
        .is_err());
        assert!(ensure_host_session(second.host_session_id().unwrap(), "").is_err());
    }
}
