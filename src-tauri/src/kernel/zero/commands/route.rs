use super::{normalize_non_empty, normalize_optional, run_command};
use crate::{errors::AppResult, models::core::CoreIpcOptions};
use serde_json::{json, Map, Value};

/// Evaluate routing, not ICMP/TTL hops. The core may resolve DNS while deciding.
pub async fn trace_route(
    target: String,
    port: u16,
    protocol: Option<String>,
    inbound_tag: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let target = normalize_non_empty(target, "target")?;
    let params = trace_route_params(target, port, protocol, inbound_tag);
    route_evaluation_result(run_command("diagnostics.trace_route", params, options).await?)
}

fn invalid_result() -> crate::errors::AppError {
    crate::errors::AppError {
        code: "invalid_response",
        message: "内核路由评估结果格式未适配；未推断规则、出口或网络健康，请核对内核契约".into(),
        details: None,
    }
}

fn route_evaluation_result(value: Value) -> AppResult<Value> {
    // zero-api declares the command envelope; its diagnostic result is Value.
    // Project only e745's actual public facts, never a guessed hop schema.
    if value.get("accepted").and_then(Value::as_bool).is_none() {
        return Err(invalid_result());
    }
    let response: zero_api::CommandResponse =
        serde_json::from_value(value).map_err(|_| invalid_result())?;
    if !response.accepted {
        return Err(crate::errors::AppError {
            code: "core_error",
            message: "内核未接受路由评估；没有生成可确认的评估结果".into(),
            details: None,
        });
    }
    let result = response.result.ok_or_else(invalid_result)?;
    let text = |key: &str, limit: usize| -> AppResult<String> {
        result[key]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= limit)
            .map(str::to_owned)
            .ok_or_else(invalid_result)
    };
    let target = text("target", 4096)?;
    let protocol = text("protocol", 16)?;
    if !matches!(protocol.as_str(), "tcp" | "udp") {
        return Err(invalid_result());
    }
    let mode = text("effective_mode", 32)?;
    if !matches!(mode.as_str(), "rule" | "direct" | "global") {
        return Err(invalid_result());
    }
    let port = result["port"]
        .as_u64()
        .filter(|p| *p <= u64::from(u16::MAX))
        .ok_or_else(invalid_result)?;
    let inbound = match result.get("inbound_tag") {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(v)) if v.len() <= 2048 => json!(v),
        _ => return Err(invalid_result()),
    };
    let action = match &result["route_action"] {
        Value::String(v) if v == "direct" || v == "reject" => json!(v),
        Value::Object(v) if v.len() == 1 => {
            let tag = v
                .get("route")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 2048)
                .ok_or_else(invalid_result)?;
            json!({"route":tag})
        }
        _ => return Err(invalid_result()),
    };
    let matched = match result.get("matched_rule") {
        Some(Value::Null) => Value::Null,
        Some(Value::Object(v)) => {
            let index = v
                .get("index")
                .and_then(Value::as_u64)
                .ok_or_else(invalid_result)?;
            let condition = v
                .get("condition")
                .and_then(Value::as_str)
                .filter(|s| s.len() <= 16384)
                .ok_or_else(invalid_result)?;
            json!({"index":index.to_string(), "condition":condition})
        }
        _ => return Err(invalid_result()),
    };
    Ok(
        json!({"target":target,"port":port,"protocol":protocol,"inbound_tag":inbound,
        "effective_mode":mode,"route_action":action,"matched_rule":matched}),
    )
}

pub(super) fn trace_route_params(
    target: String,
    port: u16,
    protocol: Option<String>,
    inbound_tag: Option<String>,
) -> Value {
    let mut params = Map::new();
    params.insert("target".to_string(), json!(target));
    params.insert("port".to_string(), json!(port));
    if let Some(protocol) = normalize_optional(protocol) {
        params.insert("protocol".to_string(), json!(protocol));
    }
    if let Some(inbound_tag) = normalize_optional(inbound_tag) {
        params.insert("inbound_tag".to_string(), json!(inbound_tag));
    }
    Value::Object(params)
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
