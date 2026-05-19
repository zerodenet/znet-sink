use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::errors::AppError;

pub const CORE_EVENT_NAME: &str = "core:event";
pub const CORE_EVENT_STATUS_NAME: &str = "core:event-status";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreEventPayload {
    pub generation: u64,
    pub event: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreEventStatus {
    pub generation: u64,
    pub status: &'static str,
    pub error: Option<AppError>,
    pub response: Option<Value>,
}

pub(crate) fn emit_core_event(app: &AppHandle, generation: u64, event: Value) {
    let _ = app.emit(CORE_EVENT_NAME, CoreEventPayload { generation, event });
}

pub(crate) fn emit_core_event_status(
    app: &AppHandle,
    generation: u64,
    status: &'static str,
    error: Option<AppError>,
    response: Option<Value>,
) {
    let _ = app.emit(
        CORE_EVENT_STATUS_NAME,
        CoreEventStatus {
            generation,
            status,
            error,
            response,
        },
    );
}
