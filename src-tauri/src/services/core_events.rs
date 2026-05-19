use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::AppHandle;

use crate::core::ipc;
use crate::errors::{AppError, AppResult};
use crate::events::emitter::{
    emit_core_event, emit_core_event_status, CORE_EVENT_NAME, CORE_EVENT_STATUS_NAME,
};
use crate::models::core::{CoreEndpoint, CoreEventSubscription, CoreIpcOptions};
use crate::services::control_plane::{endpoint_from_options, timeout_from_options};

pub fn start(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    events: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreEventSubscription> {
    let endpoint = endpoint_from_options(options.as_ref())?;
    let timeout = timeout_from_options(options.as_ref())?;

    tauri::async_runtime::spawn_blocking(move || {
        let result = subscribe_and_forward_events(
            app.clone(),
            active_generation.clone(),
            generation,
            endpoint,
            events,
            timeout,
        );

        match result {
            Ok(()) => {
                let status = if active_generation.load(Ordering::SeqCst) == generation {
                    "disconnected"
                } else {
                    "stopped"
                };
                emit_core_event_status(&app, generation, status, None, None);
            }
            Err(error) => {
                let status = if error.is_unavailable() {
                    "offline"
                } else {
                    "error"
                };
                emit_core_event_status(&app, generation, status, Some(error), None);
            }
        }
    });

    Ok(CoreEventSubscription {
        generation,
        event_name: CORE_EVENT_NAME,
        status_event_name: CORE_EVENT_STATUS_NAME,
    })
}

fn subscribe_and_forward_events(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    endpoint: CoreEndpoint,
    events: Option<Vec<String>>,
    timeout: Duration,
) -> AppResult<()> {
    let frame = match events {
        Some(events) => json!({ "type": "subscribe", "events": events }),
        None => json!({ "type": "subscribe" }),
    };
    let frame = ipc::serialize_frame(&frame)?;
    let mut stream = ipc::subscribe(endpoint, frame, timeout)?;

    let response = stream.read_next()?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::core_response(response));
    }
    emit_core_event_status(&app, generation, "subscribed", None, Some(response));

    while active_generation.load(Ordering::SeqCst) == generation {
        let event = stream.read_next()?;
        emit_core_event(&app, generation, event);
    }

    Ok(())
}
