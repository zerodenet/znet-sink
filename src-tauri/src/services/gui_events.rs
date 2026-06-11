use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::AppHandle;

use crate::kernel::protocol;
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

    let gen = Arc::clone(&active_generation);
    tauri::async_runtime::spawn_blocking(move || {
        let result = subscribe_and_forward_events(
            app.clone(),
            gen,
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
    _timeout: Duration,
) -> AppResult<()> {
    // Subscribe via the global multiplexed connection — uses the SAME
    // persistent pipe as all query/command/ping traffic.  The subscribe
    // confirmation is a regular response frame (ok:true) routed through
    // the pending-request map; events come through the broadcast channel.
    let event_types_ref: Option<Vec<String>> = event_types;
    let event_types_slice: Option<&[String]> = event_types_ref.as_ref().map(|v| v.as_slice());
    let mut rx = crate::kernel::connection::subscribe_events(
        endpoint,
        event_types_slice,
    )?;

    emit_status(&app, generation, "subscribed", None);

    loop {
        if active_generation.load(Ordering::SeqCst) != generation {
            break;
        }

        match rx.blocking_recv() {
            Ok(source_event) => {
                let event = events::normalize_event(&source_event);
                emit_gui_event(&app, GuiEventPayload { generation, event });
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                eprintln!("[ZNet] event stream lagged by {n} events, skipping");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                break;
            }
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
