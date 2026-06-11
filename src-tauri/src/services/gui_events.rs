use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::AppHandle;

use crate::kernel::{connection, protocol};
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
    // See `core_events::start`: per-type filtering is currently unused because
    // the shared multiplexed connection subscribes to every event.
    _event_types: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiEventSubscription> {
    let endpoint = protocol::endpoint_from_options(options.as_ref())?;
    let timeout = protocol::timeout_from_options(options.as_ref())?;

    let gen = Arc::clone(&active_generation);
    tauri::async_runtime::spawn_blocking(move || {
        let result =
            subscribe_and_forward_events(app.clone(), gen, generation, endpoint, timeout);

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
    timeout: Duration,
) -> AppResult<()> {
    // The multiplexed connection already sent `subscribe` when it was
    // established, so attaching as a broadcast receiver is all that's needed.
    let conn = connection::get_or_connect(endpoint, timeout)?;
    emit_status(&app, generation, "subscribed", None);

    let mut receiver = conn.subscribe_events();
    while active_generation.load(Ordering::SeqCst) == generation {
        match receiver.blocking_recv() {
            Ok(source_event) => {
                let event = events::normalize_event(&source_event);
                emit_gui_event(&app, GuiEventPayload { generation, event });
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
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
