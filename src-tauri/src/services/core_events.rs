use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::AppHandle;

use crate::errors::AppResult;
use crate::events::emitter::{
    emit_core_event, emit_core_event_status, CORE_EVENT_NAME, CORE_EVENT_STATUS_NAME,
};
use crate::kernel::{connection, protocol};
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

/// Cooldown between a lost connection and the next reconnect attempt.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(2);

fn subscribe_and_forward_events(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    endpoint: CoreEndpoint,
    timeout: Duration,
) -> AppResult<()> {
    // Reconnect loop: the multiplexed connection is torn down whenever the
    // kernel exits, so instead of giving up on the first `Closed`/connect
    // failure we back off and re-subscribe. This lets the event stream
    // self-heal after the watchdog restarts the kernel, without the
    // frontend having to re-issue `start`.
    loop {
        // Stop promptly if a newer generation took over.
        if active_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }

        let conn = match connection::get_or_connect(endpoint.clone(), timeout) {
            Ok(conn) => conn,
            Err(error) => {
                // Kernel likely down — tell the frontend, then wait for the
                // watchdog to bring it back.
                emit_core_event_status(&app, generation, "offline", Some(error), None);
                sleep_interruptible(&active_generation, generation, RECONNECT_BACKOFF);
                continue;
            }
        };
        emit_core_event_status(&app, generation, "subscribed", None, None);

        let mut receiver = conn.subscribe_events();
        let mut closed = false;
        while active_generation.load(Ordering::SeqCst) == generation {
            match receiver.blocking_recv() {
                Ok(event) => emit_core_event(&app, generation, event),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Slow consumer: some events were dropped. Keep going.
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    closed = true;
                    break;
                }
            }
        }

        // Generation advanced → the caller stopped us.
        if active_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        // Otherwise the connection was torn down (kernel crash/restart).
        if closed {
            emit_core_event_status(&app, generation, "reconnecting", None, None);
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
