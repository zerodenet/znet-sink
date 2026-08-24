use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::errors::{AppError, AppResult};
use crate::events::emitter::{
    emit_gui_event, emit_gui_event_status, GUI_EVENT_NAME, GUI_EVENT_STATUS_NAME,
};
use crate::kernel::zero::{events, queries};
use crate::kernel::{connection, protocol};
use crate::models::core::{CoreEndpoint, CoreIpcOptions};
use crate::models::gui_core::{
    GuiConnection, GuiConnectionListOptions, GuiEvent, GuiEventData, GuiEventPayload,
    GuiEventStatus, GuiEventSubscription,
};
use crate::state::app_state::AppState;

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
                emit_status(&app, generation, status, None, None);
            }
            Err(error) => {
                let status = if error.is_unavailable() {
                    "offline"
                } else {
                    "error"
                };
                emit_status(&app, generation, status, Some(error), None);
            }
        }
    });

    Ok(GuiEventSubscription {
        generation,
        event_name: GUI_EVENT_NAME,
        status_event_name: GUI_EVENT_STATUS_NAME,
    })
}

const MIN_RECONNECT_BACKOFF: Duration = Duration::from_secs(1);
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(5);
const EVENT_RECEIVER_POLL_INTERVAL: Duration = Duration::from_millis(50);
const ACTIVE_FLOW_RECONCILE_INTERVAL: Duration = Duration::from_secs(1);

fn should_retire_after_reconcile_failure(
    has_pending_requests: bool,
    received_recently: bool,
) -> bool {
    !has_pending_requests && !received_recently
}

fn subscribe_and_forward_events(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    endpoint: CoreEndpoint,
    timeout: Duration,
) -> AppResult<()> {
    let mut backoff = MIN_RECONNECT_BACKOFF;

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
                emit_status(&app, generation, "offline", Some(error), None);
                sleep_interruptible(&active_generation, generation, backoff);
                backoff = next_reconnect_backoff(backoff);
                continue;
            }
        };
        backoff = MIN_RECONNECT_BACKOFF;
        // Register before snapshot queries so events arriving during resync
        // remain buffered for this consumer instead of being dropped.
        let mut receiver = conn.subscribe_events();
        let snapshot = resync_snapshot(&app, endpoint.clone(), timeout);
        emit_status(&app, generation, "subscribed", None, snapshot);
        let mut next_active_flow_reconcile = Instant::now() + ACTIVE_FLOW_RECONCILE_INTERVAL;

        let mut closed = false;
        while active_generation.load(Ordering::SeqCst) == generation {
            // This forwarder keeps the old connection (and therefore its
            // broadcast sender) alive. The receiver cannot report `Closed`
            // merely because the shared transport was retired by the watchdog
            // or another failed IPC request. Observe transport liveness
            // explicitly and move onto the replacement subscription.
            if !conn.is_alive() {
                closed = true;
                break;
            }

            let receiver_idle = match receiver.try_recv() {
                Ok(source_event) => {
                    let event = events::normalize_event(&source_event);
                    match &event.payload {
                        GuiEventData::PolicyProbeCompleted(probe) => {
                            crate::services::probe::record_policy_probe_completed(&app, probe);
                        }
                        GuiEventData::PolicySelected(_) => {
                            // `policy.selected` is an authoritative runtime-state
                            // invalidation even when no probe-completion payload is
                            // present (notably on older Zero revisions). NodesTab
                            // listens to this existing Client Core signal and then
                            // re-queries the authoritative NodeScreen snapshot.
                            let state = app.state::<AppState>();
                            let _ = app.emit(
                                crate::services::probe::CLIENT_CORE_UPDATED_EVENT,
                                state.client_core_snapshot(),
                            );
                        }
                        GuiEventData::TrafficStats(stats) => {
                            // Zero already emits `stats.sampled` every second.
                            // Reuse the normalized typed payload for tray rates,
                            // traffic-ball updates and the one-off snapshot baseline
                            // instead of issuing a second periodic stats query.
                            crate::services::traffic_sampler::handle_stats_sample(
                                &app,
                                stats,
                                event
                                    .occurred_at_unix_ms
                                    .unwrap_or_else(crate::services::common::now_unix_ms),
                            );
                        }
                        _ => {}
                    }
                    emit_gui_event(&app, GuiEventPayload { generation, event });
                    false
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    // A lagged receiver has lost one or more flow deltas.
                    // Re-establish an authoritative baseline instead of
                    // silently leaving the live connection page stale.
                    let snapshot = resync_snapshot(&app, endpoint.clone(), timeout);
                    emit_status(&app, generation, "subscribed", None, snapshot);
                    next_active_flow_reconcile = Instant::now() + ACTIVE_FLOW_RECONCILE_INTERVAL;
                    false
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    closed = true;
                    break;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => true,
            };

            if Instant::now() >= next_active_flow_reconcile {
                // Flow pushes remain the low-latency path, but the active-flow
                // query is the authoritative state. Reconcile it periodically
                // so a dropped/quiet push stream cannot freeze the live page.
                match resync_active_connections(endpoint.clone(), timeout) {
                    Ok(connections) => {
                        emit_connection_snapshot(&app, generation, connections);
                    }
                    Err(error) if error.is_unavailable() => {
                        // A kernel may serialize a long command such as
                        // `tun.start`, causing this cheap read to hit its own
                        // shorter deadline. Never close the shared transport
                        // while another request is still legitimately pending,
                        // or while events/responses have arrived recently.
                        let received_recently =
                            conn.received_within(ACTIVE_FLOW_RECONCILE_INTERVAL + timeout);
                        if should_retire_after_reconcile_failure(
                            conn.has_pending_requests(),
                            received_recently,
                        ) {
                            conn.retire();
                            closed = true;
                            break;
                        }
                    }
                    Err(_) => {
                        // A supported kernel can still reject a particular
                        // query. Keep event delivery alive in that case.
                    }
                }
                next_active_flow_reconcile = Instant::now() + ACTIVE_FLOW_RECONCILE_INTERVAL;
            }

            // Drain bursts without adding latency or an artificial events/sec
            // ceiling. Only back off when the local broadcast queue is empty.
            if receiver_idle {
                std::thread::sleep(EVENT_RECEIVER_POLL_INTERVAL);
            }
        }

        if active_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        if closed {
            emit_status(&app, generation, "reconnecting", None, None);
            sleep_interruptible(&active_generation, generation, backoff);
            backoff = next_reconnect_backoff(backoff);
        }
    }
}

fn next_reconnect_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_RECONNECT_BACKOFF)
}

fn resync_snapshot(app: &AppHandle, endpoint: CoreEndpoint, timeout: Duration) -> Option<Value> {
    // Safe to block_on here: this runs on a `spawn_blocking` thread (see
    // `start` above), not a tokio worker — so we don't nest runtimes.
    let app = app.clone();
    tauri::async_runtime::block_on(async move {
        let options = Some(CoreIpcOptions {
            socket: Some(endpoint.path),
            timeout_ms: Some(timeout.as_millis() as u64),
        });

        let runtime =
            queries::query_value(json!({"runtime": {}}), "runtime", options.clone()).await;
        let stats = queries::query_value(json!({"stats": {}}), "stats", options.clone()).await;
        let policies =
            queries::query_value(json!({"policies": {}}), "policies", options.clone()).await;
        if let Ok(value) = &policies {
            let groups = crate::kernel::zero::parsing::parse_policy_groups(value);
            crate::services::probe::reconcile_policy_snapshot(&app, &groups);
        }
        // Query active flows last. Events are already buffered by the receiver,
        // so this snapshot becomes the baseline and subsequent buffered deltas
        // apply on top without a reconnect gap.
        let connections = queries::connections(
            Some(GuiConnectionListOptions {
                limit: Some(500),
                inbound_tag: None,
                principal_key: None,
            }),
            options,
        )
        .await;

        Some(json!({
            "runtime": runtime.ok(),
            "stats": stats.ok(),
            "policies": policies.ok(),
            "connections": connections.ok(),
        }))
    })
}

fn resync_active_connections(
    endpoint: CoreEndpoint,
    timeout: Duration,
) -> AppResult<Vec<GuiConnection>> {
    tauri::async_runtime::block_on(async move {
        let options = Some(CoreIpcOptions {
            socket: Some(endpoint.path),
            timeout_ms: Some(timeout.as_millis() as u64),
        });
        queries::connections(
            Some(GuiConnectionListOptions {
                limit: Some(500),
                inbound_tag: None,
                principal_key: None,
            }),
            options,
        )
        .await
        .map(|connections| connections.items)
    })
}

fn emit_connection_snapshot(app: &AppHandle, generation: u64, connections: Vec<GuiConnection>) {
    emit_gui_event(
        app,
        GuiEventPayload {
            generation,
            event: GuiEvent {
                event_type: "connection.snapshot".to_string(),
                source_event_type: "gui.activeFlowsReconcile".to_string(),
                event_id: None,
                sequence: None,
                occurred_at_unix_ms: Some(crate::services::common::now_unix_ms()),
                payload: GuiEventData::Connections(connections),
            },
        },
    );
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

fn emit_status(
    app: &AppHandle,
    generation: u64,
    status: &'static str,
    error: Option<AppError>,
    response: Option<Value>,
) {
    if status != "subscribed" {
        crate::services::traffic_sampler::clear_runtime_traffic_state(app);
    }
    emit_gui_event_status(
        app,
        GuiEventStatus {
            generation,
            status,
            error,
            response,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::should_retire_after_reconcile_failure;

    #[test]
    fn reconcile_timeout_never_retires_a_busy_or_recent_connection() {
        assert!(!should_retire_after_reconcile_failure(true, false));
        assert!(!should_retire_after_reconcile_failure(false, true));
        assert!(!should_retire_after_reconcile_failure(true, true));
        assert!(should_retire_after_reconcile_failure(false, false));
    }
}
