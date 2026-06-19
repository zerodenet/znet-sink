use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::AppHandle;

use crate::errors::{AppError, AppResult};
use crate::events::emitter::{
    emit_gui_event, emit_gui_event_status, GUI_EVENT_NAME, GUI_EVENT_STATUS_NAME,
};
use crate::kernel::zero::events;
use crate::kernel::{connection, protocol};
use crate::models::core::{CoreEndpoint, CoreIpcOptions};
use crate::models::gui_core::{GuiEventPayload, GuiEventStatus, GuiEventSubscription};

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
        let result = subscribe_and_forward_events(app.clone(), gen, generation, endpoint, timeout);

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

/// Cooldown between a lost connection and the next reconnect attempt.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(2);

fn subscribe_and_forward_events(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    endpoint: CoreEndpoint,
    timeout: Duration,
) -> AppResult<()> {
    // Reconnect loop — see `core_events::subscribe_and_forward_events` for
    // the rationale. Lets the GUI event stream self-heal after the watchdog
    // restarts the kernel.
    loop {
        if active_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }

        let conn = match connection::get_or_connect(endpoint.clone(), timeout) {
            Ok(conn) => conn,
            Err(error) => {
                emit_status(&app, generation, "offline", Some(error));
                sleep_interruptible(&active_generation, generation, RECONNECT_BACKOFF);
                continue;
            }
        };
        emit_status(&app, generation, "subscribed", None);

        let mut receiver = conn.subscribe_events();
        let mut closed = false;
        while active_generation.load(Ordering::SeqCst) == generation {
            match receiver.blocking_recv() {
                Ok(source_event) => {
                    let event = events::normalize_event(&source_event);
                    emit_gui_event(&app, GuiEventPayload { generation, event });
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    closed = true;
                    break;
                }
            }
        }

        if active_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        if closed {
            emit_status(&app, generation, "reconnecting", None);
            sleep_interruptible(&active_generation, generation, RECONNECT_BACKOFF);
        }
    }
}

/// Sleep for `total`, waking early if the subscription generation is
/// superseded — so `stop` takes effect promptly instead of waiting out the
/// full reconnect backoff.
fn sleep_interruptible(active_generation: &AtomicU64, generation: u64, total: Duration) {
    let step = Duration::from_millis(200);
    let mut waited = Duration::ZERO;
    while waited < total {
        if active_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let sleep = step.min(total - waited);
        std::thread::sleep(sleep);
        waited += sleep;
    }
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
