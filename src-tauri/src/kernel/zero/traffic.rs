//! Traffic Observation V1 wire boundary over the existing local IPC transport.
use crate::errors::{AppError, AppResult};
use crate::kernel::protocol;
use crate::models::core::CoreIpcOptions;
use serde_json::{json, Value};
#[cfg(test)]
mod tests;
pub(crate) mod wire;

pub(crate) async fn query(
    request: Value,
    variant: &str,
    options: CoreIpcOptions,
) -> AppResult<Value> {
    let value = call(json!({"type":"query","request":request}), options).await?;
    value
        .get(variant)
        .cloned()
        .ok_or_else(|| AppError::internal("invalid traffic IPC query variant"))
}
pub(crate) async fn reset(params: Value, options: CoreIpcOptions) -> AppResult<Value> {
    let value = call(
        json!({"type":"command","method":"stats.reset","params":params}),
        options,
    )
    .await?;
    if value["accepted"] != true {
        return Err(AppError::core_response(value));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| AppError::internal("missing statistics reset acknowledgement"))
}
async fn call(frame: Value, options: CoreIpcOptions) -> AppResult<Value> {
    let call = protocol::request(frame.clone(), Some(options.clone())).await;
    let result = match call {
        Ok(call) => super::parsing::unwrap_call_result(call.response, call.error),
        Err(error) => Err(error),
    };
    // Only an explicit refusal of the initial subscribe handshake proves that
    // the requested query/command has not been sent. Never replay a mutation
    // after a timeout, transport close or an ambiguous acknowledgement.
    match result {
        Err(error) if subscription_refused(&error) => {
            let call = protocol::request_single_shot(frame, Some(options)).await?;
            super::parsing::unwrap_call_result(call.response, call.error)
        }
        value => value,
    }
}
fn subscription_refused(error: &AppError) -> bool {
    matches!(
        error.code,
        "unsupported" | "permission_denied" | "core_response"
    ) && error.details.as_ref().is_some_and(|details| {
        details["id"] == "znet-sink-subscribe"
            && details["ok"] == false
            && matches!(
                details["error"]["code"].as_str(),
                Some("unsupported" | "permission_denied")
            )
    })
}
