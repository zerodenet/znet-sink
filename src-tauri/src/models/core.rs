use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::AppError;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreIpcOptions {
    pub socket: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreEndpoint {
    pub transport: &'static str,
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreEventSubscription {
    pub generation: u64,
    pub event_name: &'static str,
    pub status_event_name: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreCallResult {
    pub available: bool,
    pub endpoint: CoreEndpoint,
    pub request_id: Option<Value>,
    pub response_id: Option<Value>,
    pub response: Option<Value>,
    pub error: Option<AppError>,
}

impl CoreCallResult {
    pub(crate) fn from_core_result(
        endpoint: CoreEndpoint,
        request_id: Option<Value>,
        result: Result<Value, AppError>,
    ) -> CoreCallResult {
        match result {
            Ok(response) => {
                let response_id = response_id(&response);
                CoreCallResult {
                    available: true,
                    endpoint,
                    request_id,
                    response_id,
                    response: Some(response),
                    error: None,
                }
            }
            Err(error) => CoreCallResult {
                available: !error.is_unavailable(),
                endpoint,
                request_id,
                response_id: None,
                response: None,
                error: Some(error),
            },
        }
    }
}

pub(crate) fn response_id(response: &Value) -> Option<Value> {
    response
        .get("id")
        .or_else(|| response.get("request_id"))
        .or_else(|| response.get("requestId"))
        .cloned()
}
