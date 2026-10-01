//! Desktop ownership and feature admission; the kernel owns reset authorization.
use crate::errors::{AppError, AppResult};
use crate::kernel::zero::{parsing, traffic};
use crate::models::core::CoreIpcOptions;
use serde_json::{json, Value};
mod types;
pub use types::{ListInput, ResetInput, Scope};
#[cfg(all(test, unix))]
mod ipc_tests;
#[cfg(test)]
mod tests;

pub async fn discover(options: CoreIpcOptions) -> AppResult<Value> {
    let raw = traffic::query(json!({"capabilities":{}}), "capabilities", options).await?;
    let caps = parsing::parse_capabilities(&raw, None);
    let supported = caps.available
        && raw["traffic_statistics"]["contract_version"] == 1
        && raw["features"]
            .as_array()
            .is_some_and(|v| v.iter().any(|f| f == "traffic_observation_v1"));
    // Existing Zero local IPC is authenticated by OS socket/pipe access and
    // assigns ipc-local Admin. capabilities.permissions is adapter metadata,
    // not the caller's grants. The kernel still validates each command.
    Ok(json!({"capabilities":caps,"supported":supported,"transport":"ipc", "admin":true}))
}
pub async fn page(input: ListInput, options: CoreIpcOptions) -> AppResult<Value> {
    let value = traffic::query(
        json!({"traffic_stats":input.wire()?}),
        "traffic_stats",
        options,
    )
    .await?;
    let scopes = value["scopes"]
        .as_array()
        .ok_or_else(|| AppError::internal("invalid traffic list"))?;
    for snapshot in scopes {
        traffic::wire::snapshot(snapshot)?;
    }
    Ok(traffic::wire::exact(value))
}
pub async fn get(scope: Scope, options: CoreIpcOptions) -> AppResult<Value> {
    let value = traffic::query(
        json!({"traffic_stat":{"scope":scope.wire()}}),
        "traffic_stat",
        options,
    )
    .await?;
    traffic::wire::snapshot(&value)?;
    Ok(traffic::wire::exact(value))
}
pub async fn reset(input: ResetInput, options: CoreIpcOptions) -> AppResult<Value> {
    let discovery = discover(options.clone()).await?;
    let caps = &discovery["capabilities"];
    let features = caps["features"].as_array();
    if discovery["supported"] != true
        || !features.is_some_and(|v| v.iter().any(|f| f == "traffic_period_reset_v1"))
        || caps["trafficStatistics"]["reset_command"] != "stats.reset"
        || caps["trafficStatistics"]["reset_permission"] != "admin"
        || caps["trafficStatistics"]["reset_policy"] != "all_available_cumulative_no_cascade"
    {
        return Err(AppError {
            code: "unsupported",
            message: "当前内核未声明兼容的统计清空能力".into(),
            details: None,
        });
    }
    let maximum = caps["trafficStatistics"]["maximum_reset_targets"]
        .as_u64()
        .unwrap_or(0)
        .min(256);
    if input.targets.len() as u64 > maximum {
        return Err(AppError::invalid_argument(
            "statistics reset target limit exceeded",
        ));
    }
    let scopes = caps["trafficStatistics"]["resettable_scopes"].as_array();
    if input
        .targets
        .iter()
        .any(|target| !scopes.is_some_and(|scopes| scopes.contains(&target.scope.wire()["kind"])))
    {
        return Err(AppError::invalid_argument(
            "statistics scope is not declared resettable",
        ));
    }
    let result = traffic::reset(input.wire()?, options).await?;
    validate_reset(&result, &input)?;
    Ok(traffic::wire::exact(result))
}
fn validate_reset(result: &Value, input: &ResetInput) -> AppResult<()> {
    let snapshots = result["snapshots"]
        .as_array()
        .ok_or_else(|| AppError::internal("invalid statistics reset acknowledgement"))?;
    if result["core_instance_id"] != input.expected_core_instance_id
        || result["operation_id"] != input.operation_id
        || snapshots.len() != input.targets.len()
    {
        return Err(AppError::internal(
            "statistics reset acknowledgement scope mismatch",
        ));
    }
    let mut found = std::collections::HashSet::new();
    for snapshot in snapshots {
        traffic::wire::snapshot(snapshot)?;
        let target = input
            .targets
            .iter()
            .find(|target| target.scope.wire() == snapshot["scope"])
            .ok_or_else(|| AppError::internal("unexpected reset scope"))?;
        if !found.insert(&target.scope)
            || snapshot["core_instance_id"] != input.expected_core_instance_id
            || snapshot["stats_epoch"] == target.expected_stats_epoch
            || target
                .expected_generation
                .as_ref()
                .is_some_and(|generation| {
                    snapshot["generation"]
                        .as_u64()
                        .map(|v| v.to_string())
                        .as_ref()
                        != Some(generation)
                })
        {
            return Err(AppError::internal(
                "invalid reset instance, period or generation",
            ));
        }
    }
    Ok(())
}
