//! Zero's neutral endpoint contract. No protocol-specific lifecycle logic.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, time::Duration};

mod direction_transition;
pub(crate) mod managed;
mod wire;
use wire::{confirmed, page_next, validate_endpoint};

use crate::errors::{AppError, AppResult};
use crate::kernel::zero::queries;
use crate::models::{core::CoreIpcOptions, gui_core::GuiZeroCapabilities};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Directions {
    pub inbound: bool,
    pub outbound: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SetState {
        enabled: bool,
        persistence: Persistence,
    },
    SetDirections {
        directions: Directions,
        persistence: Persistence,
    },
    Restart {},
    ClearOverrides {},
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    RuntimeOnly,
    SourceFile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControlInput {
    pub endpoint_id: String,
    pub core_instance_id: String,
    pub expected_intent_revision: u64,
    pub action: Action,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub capabilities: GuiZeroCapabilities,
    pub endpoints: Vec<Value>,
}

fn supports(caps: &GuiZeroCapabilities, feature: &str) -> bool {
    caps.available
        && caps.contracts.as_ref().is_some_and(|contracts| {
            contracts.capabilities.minimum_supported == 1
                && contracts.capabilities.current >= 1
                && contracts.control_api.minimum_supported == 1
                && contracts.control_api.current >= 1
        })
        && caps.features.iter().any(|item| item == feature)
}

fn unsupported(message: &str) -> AppError {
    AppError {
        code: "unsupported",
        message: message.into(),
        details: None,
    }
}

pub async fn catalog(options: CoreIpcOptions) -> AppResult<Catalog> {
    let capabilities = queries::zero_capabilities(Some(options.clone())).await?;
    // Old kernels are still usable; never probe an undeclared query variant.
    if !supports(&capabilities, "network_endpoint_catalog_v1") {
        return Ok(Catalog {
            capabilities,
            endpoints: Vec::new(),
        });
    }
    let mut endpoints = Vec::new();
    let mut offset = 0;
    let mut scope = None;
    let mut identities = HashSet::new();
    loop {
        let page = queries::query_value(
            json!({"endpoints":{"offset":offset,"limit":1000}}),
            "endpoints",
            Some(options.clone()),
        )
        .await?;
        let rows = page["endpoints"]
            .as_array()
            .ok_or_else(|| AppError::internal("invalid endpoint catalog"))?;
        for row in rows {
            validate_endpoint(row)?;
            if !identities.insert(row["endpoint_id"].as_str().unwrap().to_owned()) {
                return Err(AppError::internal("duplicate endpoint identity in catalog"));
            }
            let current = (
                row["core_instance_id"].clone(),
                row["config_revision"].clone(),
            );
            if let Some(previous) = &scope {
                if previous != &current {
                    return Err(AppError::conflict(
                        "endpoint_catalog",
                        "",
                        "内核或配置已变化，请刷新端点目录",
                    ));
                }
            } else {
                scope = Some(current);
            }
            endpoints.push(row.clone());
        }
        match page_next(&page, offset, rows.len())? {
            None => break,
            Some(next) => offset = next,
        }
    }
    Ok(Catalog {
        capabilities,
        endpoints,
    })
}

pub async fn details(id: String, instance: String, options: CoreIpcOptions) -> AppResult<Value> {
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    if !supports(&caps, "network_endpoint_catalog_v1") {
        return Err(unsupported("当前内核未声明端点目录能力"));
    }
    let current = get(&id, options.clone()).await?;
    ensure_instance(&current, &instance)?;
    let result = queries::query_value(
        json!({"endpoint_details":{"endpoint_id":id}}),
        "endpoint_details",
        Some(options.clone()),
    )
    .await?;
    let after = get(&id, options).await?;
    ensure_instance(&after, &instance)?;
    if current["config_revision"] != after["config_revision"]
        || current["generation"] != after["generation"]
        || result["generation"] != after["generation"]
    {
        return Err(AppError::conflict(
            "endpoint",
            id,
            "端点实例已变化，请刷新详情",
        ));
    }
    Ok(result)
}

async fn get(id: &str, options: CoreIpcOptions) -> AppResult<Value> {
    queries::query_value(
        json!({"endpoint":{"endpoint_id":id}}),
        "endpoint",
        Some(options),
    )
    .await
}

fn ensure_instance(endpoint: &Value, instance: &str) -> AppResult<()> {
    if instance.is_empty() || endpoint["core_instance_id"].as_str() != Some(instance) {
        return Err(AppError::conflict(
            "endpoint",
            "",
            "内核已更换，旧端点操作已取消，请刷新",
        ));
    }
    Ok(())
}

pub fn control_params(
    input: &ControlInput,
    current: &Value,
    caps: &GuiZeroCapabilities,
) -> AppResult<(&'static str, Value)> {
    if !supports(caps, "network_endpoint_control_v1") {
        return Err(unsupported("当前内核未声明端点控制能力"));
    }
    ensure_instance(current, &input.core_instance_id)?;
    if current["endpoint_id"].as_str() != Some(&input.endpoint_id)
        || current["intent_revision"].as_u64() != Some(input.expected_intent_revision)
    {
        return Err(AppError::conflict(
            "endpoint",
            &input.endpoint_id,
            "端点意图已变化，请刷新后重试",
        ));
    }
    let mut params = json!({"endpoint_id":input.endpoint_id,"expected_intent_revision":input.expected_intent_revision});
    if supports(caps, "network_endpoint_control_preconditions_v1") {
        params["expected_core_instance_id"] = json!(input.core_instance_id);
    }
    let (operation, method) = match &input.action {
        Action::SetState {
            enabled,
            persistence,
        } => {
            params["enabled"] = json!(enabled);
            params["persistence"] = json!(persistence);
            ("set_state", "endpoints.set_state")
        }
        Action::SetDirections {
            directions,
            persistence,
        } => {
            if (directions.inbound
                && current
                    .pointer("/supported/directions/inbound")
                    .and_then(Value::as_bool)
                    != Some(true))
                || (directions.outbound
                    && current
                        .pointer("/supported/directions/outbound")
                        .and_then(Value::as_bool)
                        != Some(true))
            {
                return Err(unsupported("该端点没有配置此方向的执行能力"));
            }
            params["directions"] = json!(directions);
            params["persistence"] = json!(persistence);
            ("set_directions", "endpoints.set_directions")
        }
        Action::Restart {} => ("restart", "endpoints.restart"),
        Action::ClearOverrides {} => ("clear_overrides", "endpoints.clear_overrides"),
    };
    if !current
        .pointer("/supported/operations")
        .and_then(Value::as_array)
        .is_some_and(|ops| ops.iter().any(|op| op.as_str() == Some(operation)))
    {
        return Err(unsupported("端点协议未注册此控制操作"));
    }
    Ok((method, params))
}

pub async fn control(input: ControlInput, options: CoreIpcOptions) -> AppResult<Value> {
    control_at_revision(input, options, None).await
}

async fn control_at_revision(
    input: ControlInput,
    options: CoreIpcOptions,
    revision: Option<u64>,
) -> AppResult<Value> {
    let caps = queries::zero_capabilities(Some(options.clone())).await?;
    if !supports(&caps, "network_endpoint_control_v1") {
        return Err(unsupported("当前内核未声明端点控制能力"));
    }
    let current = get(&input.endpoint_id, options.clone()).await?;
    if revision.is_some_and(|revision| current["config_revision"].as_u64() != Some(revision)) {
        return Err(AppError::conflict(
            "endpoint",
            &input.endpoint_id,
            "端点配置已变化，请刷新",
        ));
    }
    let (method, params) = control_params(&input, &current, &caps)?;
    // Kernel reconciliation and rollback each have a bounded 15s wait. Keep
    // connection deadlines short, allow both phases to finish before reporting.
    let call = crate::kernel::protocol::command_with_response_timeout(
        method.into(),
        Some(params),
        Some(options),
        Some(Duration::from_secs(40)),
    )
    .await?;
    let response = crate::kernel::zero::parsing::unwrap_call_result(call.response, call.error)?;
    confirmed(response, &input)
}

#[cfg(test)]
#[path = "network_endpoint/tests.rs"]
mod tests;
