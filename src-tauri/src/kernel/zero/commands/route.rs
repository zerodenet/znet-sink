use super::{normalize_non_empty, normalize_optional, run_command};
use crate::{errors::AppResult, models::core::CoreIpcOptions};
use serde_json::{json, Map, Value};

/// Route trace diagnostic.
pub async fn trace_route(
    target: String,
    port: u16,
    protocol: Option<String>,
    inbound_tag: Option<String>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Value> {
    let target = normalize_non_empty(target, "target")?;
    let params = trace_route_params(target, port, protocol, inbound_tag);
    run_command("diagnostics.trace_route", params, options).await
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
