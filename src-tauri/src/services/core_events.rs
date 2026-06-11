use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::AppHandle;

use crate::kernel::{connection, protocol};
use crate::errors::AppResult;
use crate::events::emitter::{
    CORE_EVENT_NAME, CORE_EVENT_STATUS_NAME, emit_core_event, emit_core_event_status,
};
use crate::models::core::{CoreEndpoint, CoreEventSubscription, CoreIpcOptions};

pub fn start(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    // NOTE: per-type event filtering used to be delegated to the kernel's
    // `subscribe` frame. The multiplexed connection now subscribes to every
    // event (it is shared across consumers), so this filter is currently
    // unused — retained for API compatibility with the command layer.
    _events: Option<Vec<String>>,
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
    timeout: Duration,
) -> AppResult<()> {
    // The multiplexed connection already sent `subscribe` when it was
    // established, so attaching as a broadcast receiver is all that's needed.
    let conn = connection::get_or_connect(endpoint, timeout)?;
    emit_core_event_status(&app, generation, "subscribed", None, None);

    let mut receiver = conn.subscribe_events();
    while active_generation.load(Ordering::SeqCst) == generation {
        match receiver.blocking_recv() {
            Ok(event) => emit_core_event(&app, generation, event),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                // Slow consumer: some events were dropped. Keep going.
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                // Connection torn down — the outer status will reflect it.
                break;
            }
        }
    }

    Ok(())
}
