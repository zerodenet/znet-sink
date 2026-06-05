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
    GUI_EVENT_NAME, GUI_EVENT_STATUS_NAME, emit_gui_event, emit_gui_event_status,
};
use crate::models::core::{CoreEndpoint, CoreIpcOptions};
use crate::models::gui_core::{GuiEventPayload, GuiEventStatus, GuiEventSubscription};
use crate::kernel::zero::events;

pub fn start(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    event_types: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiEventSubscription> {
    let endpoint = protocol::endpoint_from_options(options.as_ref())?;
    let timeout = protocol::timeout_from_options(options.as_ref())?;

    tauri::async_runtime::spawn_blocking(move || {
        let result = subscribe_and_forward_events(
            app.clone(),
            active_generation.clone(),
            generation,
            endpoint,
            event_types,
            timeout,
        );

        match result {
            Ok(()) => {
                let status = if active_generation.load(Ordering::SeqCst) == generation {
                    "disconnected"
                } else {
                    "stopped"
                };
                emit_status(&app, generation, status, None);
            }
            Err(error) => {
                let status = if error.is_unavailable() {
                    "offline"
                } else {
                    "error"
                };
                emit_status(&app, generation, status, Some(error));
            }
        }
    });

    Ok(GuiEventSubscription {
        generation,
        event_name: GUI_EVENT_NAME,
        status_event_name: GUI_EVENT_STATUS_NAME,
    })
}

fn subscribe_and_forward_events(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    endpoint: CoreEndpoint,
    event_types: Option<Vec<String>>,
    timeout: Duration,
) -> AppResult<()> {
    let frame = match event_types {
        Some(types) => serde_json::json!({ "type": "subscribe", "events": types }),
        None => serde_json::json!({ "type": "subscribe" }),
    };
    let frame = transport::serialize_frame(&frame)?;
    let mut stream = transport::subscribe(endpoint, frame, timeout)?;

    let response = stream.read_next()?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(AppError::core_response(response));
    }
    emit_status(&app, generation, "subscribed", None);

    while active_generation.load(Ordering::SeqCst) == generation {
        let source_event = stream.read_next()?;
        let event = events::normalize_event(&source_event);
        emit_gui_event(&app, GuiEventPayload { generation, event });
    }

    Ok(())
}

fn emit_status(app: &AppHandle, generation: u64, status: &'static str, error: Option<AppError>) {
    emit_gui_event_status(
        app,
        GuiEventStatus {
            generation,
            status,
            error,
        },
    );
}
