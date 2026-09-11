use serde_json::json;

use crate::errors::{AppError, AppResult};
use crate::kernel::configuration::BoundControl;
use crate::models::core::CoreIpcOptions;
use crate::models::core_process::CoreProcessState;
use crate::models::logs::LogLevel;
use crate::services::common::lock;
use crate::services::{core_config, core_process, logs};
use crate::state::app_state::AppState;

#[derive(Clone)]
pub(crate) struct FlowBoundary {
    core_instance_id: String,
    flow_ids: Vec<String>,
    control: BoundControl,
}

pub(crate) async fn capture(
    state: &AppState,
    target_id: &str,
) -> AppResult<Option<(CoreIpcOptions, FlowBoundary)>> {
    let active_id = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .map(|profile| profile.id.clone());
    if active_id.as_deref() == Some(target_id) {
        return Ok(None);
    }

    if core_process::refresh_status(state)?.state != CoreProcessState::Running {
        return Ok(None);
    }

    let options = {
        let config = lock(state.app_config(), "app_config")?;
        core_config::ipc_options_from_app_config(&config.core)
    };
    let boundary = capture_flow_boundary(options.clone())
        .await
        .map_err(|error| {
            AppError::internal(format!(
                "cannot establish a connection boundary before switching proxy config: {}",
                error.message
            ))
        })?;

    Ok(Some((options, boundary)))
}

pub(crate) async fn reconcile(state: &AppState, _options: CoreIpcOptions, boundary: FlowBoundary) {
    let report = znet_engine_client::flow_cleanup::close_previous(
        &boundary.control,
        &boundary.core_instance_id,
        &boundary.flow_ids,
    )
    .await;
    if !report.failures.is_empty() || report.boundary_error.is_some() {
        let failures: Vec<_> = report
            .failures
            .iter()
            .map(|(id, error)| json!({"flowId":id,"code":error.code,"message":error.message}))
            .collect();
        logs::znet_log_fields(
            Some(state),
            LogLevel::Warn,
            "proxy config switched; previous connection cleanup is incomplete",
            json!({"coreInstanceId":boundary.core_instance_id,"closedCount":report.closed,
                "skippedCount":report.skipped,"failures":failures,"boundaryError":report.boundary_error}),
        );
    }
}

async fn capture_flow_boundary(options: CoreIpcOptions) -> AppResult<FlowBoundary> {
    let control = BoundControl::connect(options).await?;
    let before = control.identity().await?;
    let flow_ids = control.all_flow_ids().await?;
    let after = control.identity().await?;
    if before != after {
        return Err(AppError::conflict(
            "config",
            "runtime",
            "runtime changed while capturing previous flows",
        ));
    }
    Ok(FlowBoundary {
        core_instance_id: before.core_instance_id,
        flow_ids,
        control,
    })
}
