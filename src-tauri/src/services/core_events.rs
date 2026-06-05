use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::AppHandle;

use crate::kernel::{protocol, transport};
use crate::errors::{AppError, AppResult};
use crate::events::emitter::{
    CORE_EVENT_NAME, CORE_EVENT_STATUS_NAME, emit_core_event, emit_core_event_status,
};
use crate::models::core::{CoreEndpoint, CoreEventSubscription, CoreIpcOptions};

pub fn start(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    events: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<CoreEventSubscription> {
    let endpoint = protocol::endpoint_from_options(options.as_ref())?;
    let timeout = protocol::timeout_from_options(options.as_ref())?;

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
        Some(events) => serde_json::json!({ "type": "subscribe", "events": events }),
        None => serde_json::json!({ "type": "subscribe" }),
    };
    let frame = transport::serialize_frame(&frame)?;
    let mut stream = transport::subscribe(endpoint, frame, timeout)?;

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
