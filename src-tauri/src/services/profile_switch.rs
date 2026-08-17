use serde_json::{json, Value};

use crate::errors::{AppError, AppResult};
use crate::kernel::zero::{commands as zero_commands, parsing, queries};
use crate::models::core::CoreIpcOptions;
use crate::models::core_process::CoreProcessState;
use crate::models::logs::LogLevel;
use crate::services::common::lock;
use crate::services::{core_config, core_process, logs};
use crate::state::app_state::AppState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FlowBoundary {
    core_instance_id: String,
    flow_ids: Vec<String>,
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

pub(crate) async fn reconcile(
    state: &AppState,
    options: CoreIpcOptions,
    boundary: FlowBoundary,
) {
    if boundary.flow_ids.is_empty() {
        return;
    }

    let current_instance = match core_instance_id(options.clone()).await {
        Ok(instance_id) => instance_id,
        Err(error) => {
            logs::znet_log_fields(
                Some(state),
                LogLevel::Warn,
                "proxy config switched but previous connection cleanup was skipped because the current core instance could not be verified",
                json!({
                    "previousCoreInstanceId": boundary.core_instance_id,
                    "flowCount": boundary.flow_ids.len(),
                    "error": error.message,
                }),
            );
            return;
        }
    };

    // A fallback kernel restart already destroys every old flow. Flow IDs may
    // be reused by the new core instance, so never replay old close commands
    // across an instance boundary.
    if current_instance != boundary.core_instance_id {
        return;
    }

    let total = boundary.flow_ids.len();
    let mut failed = Vec::new();
    for flow_id in boundary.flow_ids {
        if let Err(error) = zero_commands::close_connection(flow_id.clone(), Some(options.clone())).await {
            failed.push(json!({
                "flowId": flow_id,
                "code": error.code,
                "message": error.message,
            }));
        }
    }

    if !failed.is_empty() {
        logs::znet_log_fields(
            Some(state),
            LogLevel::Warn,
            "proxy config switched but some previous connections could not be closed",
            json!({
                "coreInstanceId": current_instance,
                "flowCount": total,
                "failedCount": failed.len(),
                "failures": failed,
            }),
        );
    }
}

async fn capture_flow_boundary(options: CoreIpcOptions) -> AppResult<FlowBoundary> {
    let core_instance_id = core_instance_id(options.clone()).await?;
    let value = queries::query_value(active_flows_request(), "active_flows", Some(options)).await?;

    Ok(FlowBoundary {
        core_instance_id,
        flow_ids: flow_ids_from_value(&value),
    })
}

async fn core_instance_id(options: CoreIpcOptions) -> AppResult<String> {
    let runtime = queries::query_value(json!({"runtime": {}}), "runtime", Some(options)).await?;
    parsing::string_at(&runtime, &["core_instance_id", "coreInstanceId"])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::internal("core runtime did not expose core_instance_id"))
}

fn active_flows_request() -> Value {
    json!({
        "active_flows": {
            "filter": {}
        }
    })
}

fn flow_ids_from_value(value: &Value) -> Vec<String> {
    parsing::parse_connection_list(value, u32::MAX)
        .items
        .into_iter()
        .map(|connection| connection.flow_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{active_flows_request, flow_ids_from_value};

    #[test]
    fn profile_switch_snapshot_is_not_limited_by_the_ui_page_size() {
        let request = active_flows_request();
        let active = request["active_flows"]
            .as_object()
            .expect("active_flows request should be an object");

        assert_eq!(active.get("filter"), Some(&json!({})));
        assert!(!active.contains_key("limit"));
    }

    #[test]
    fn profile_switch_snapshot_extracts_all_returned_flow_ids() {
        let ids = flow_ids_from_value(&json!([
            { "record": { "flow_id": "old-1", "network": "tcp", "target": { "host": "a.example", "port": 443 } } },
            { "record": { "flow_id": "old-2", "network": "udp", "target": { "host": "8.8.8.8", "port": 53 } } }
        ]));

        assert_eq!(ids, vec!["old-1".to_string(), "old-2".to_string()]);
    }
}
