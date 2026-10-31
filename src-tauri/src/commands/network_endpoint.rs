use crate::errors::AppResult;
use crate::models::core::CoreIpcOptions;
use crate::services::{
    common, core_config,
    network_endpoint::{
        self,
        managed::{self, ManagedCatalog, ManagedInput},
    },
};
use crate::state::app_state::AppState;
use serde_json::Value;
use tauri::State;

fn options(state: &AppState) -> AppResult<CoreIpcOptions> {
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEndpointLibrary {
    protocols: Vec<Value>,
    revision: String,
    endpoints: Vec<Value>,
}
fn library(rows: Vec<Value>) -> AppResult<UserEndpointLibrary> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(&rows)
        .map_err(|_| crate::errors::AppError::internal("端点版本计算失败"))?;
    Ok(UserEndpointLibrary {
        protocols: crate::configuration::user_endpoints::protocols(),
        revision: format!("{:x}", Sha256::digest(bytes)),
        endpoints: rows,
    })
}
#[tauri::command]
pub async fn gui_user_endpoints(state: State<'_, AppState>) -> AppResult<UserEndpointLibrary> {
    let _operation = state.proxy_config_operation().lock().await;
    library(
        common::lock(state.app_config(), "app_config")?
            .user_endpoints
            .clone(),
    )
}
#[tauri::command]
pub async fn gui_user_endpoints_save(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    expected_revision: String,
    endpoints: Vec<Value>,
) -> AppResult<UserEndpointLibrary> {
    use crate::{
        errors::AppError,
        kernel::{adapter::KernelAdapter, zero::adapter::ZeroAdapter},
        models::core_process::CoreProcessState,
        services::{core_process, rule_overlay},
    };
    let _operation = state.proxy_config_operation().lock().await;
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    if library(previous.user_endpoints.clone())?.revision != expected_revision {
        return Err(AppError::conflict(
            "user_endpoints",
            "global",
            "用户端点已变化，请刷新后重新编辑",
        ));
    }
    crate::configuration::user_endpoints::validate(&endpoints)?;
    let mut next = previous.clone();
    next.user_endpoints = endpoints;
    // Compose against raw material, including while offline. Never write the
    // merged definitions back into the subscription or active source profile.
    let (id, source) = crate::configuration::local_edits::context(state.inner())?;
    {
        let profiles = common::lock(state.proxy_configs(), "proxy_config")?;
        for profile in profiles.iter() {
            if let Some(source) = &profile.content {
                crate::configuration::user_endpoints::check_removed(
                    &previous.user_endpoints,
                    &next.user_endpoints,
                    source,
                )?;
            }
        }
    }
    let candidate =
        rule_overlay::compose_candidate_with_app(state.inner(), &source, id.as_deref(), &next)?;
    crate::configuration::user_endpoints::check_removed(
        &previous.user_endpoints,
        &next.user_endpoints,
        &candidate.config,
    )?;
    if core_process::refresh_status(state.inner())?.state == CoreProcessState::Running {
        if id.is_none() {
            return Err(AppError::invalid_argument("运行中的内核需要先选择配置"));
        }
        let result = ZeroAdapter::new()
            .validate_config(candidate.config, options(state.inner())?)
            .await?;
        crate::commands::config_workspace::ensure_validation_accepted(&result)?;
    }
    let saved =
        crate::commands::app_config::apply_candidate(app_handle, state.clone(), previous, next)
            .await?;
    library(saved.user_endpoints)
}
