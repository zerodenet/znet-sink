use serde_json::Value;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use znet_engine_client::Subscription;

use crate::errors::{AppError, AppResult};
use crate::events::emitter::{
    emit_gui_event, emit_gui_event_status, GUI_EVENT_NAME, GUI_EVENT_STATUS_NAME,
};
use crate::kernel::connection;
use crate::kernel::observation::FlowObservation;
use crate::kernel::zero::events;
use crate::models::gui_core::{
    GuiConnection, GuiConnectionListOptions, GuiEvent, GuiEventData, GuiEventPayload,
    GuiEventStatus, GuiEventSubscription,
};
use crate::state::app_state::AppState;

pub(crate) fn stop(state: &AppState) -> u64 {
    let (generation, _binding) = state.observations().stop();
    generation
}

pub fn start(
    app: AppHandle,
    subscription: Subscription,
    event_types: Option<Vec<String>>,
) -> AppResult<GuiEventSubscription> {
    let filter = connection::event_filter::EventFilter::new(event_types)?;
    let generation = subscription.generation;
    tauri::async_runtime::spawn_blocking(move || {
        let result = subscribe_and_forward_events(app.clone(), &subscription, &filter);

        match result {
            Ok(()) => {
                let status = if subscription.is_current() {
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
    subscription: &Subscription,
    filter: &connection::event_filter::EventFilter,
) -> AppResult<()> {
    let generation = subscription.generation;
    let endpoint = subscription.binding.endpoint.clone();
    let timeout = subscription.binding.timeout;
    let mut backoff = MIN_RECONNECT_BACKOFF;

    // Reconnect loop — see `core_events::subscribe_and_forward_events` for
    // the rationale. Lets the GUI event stream self-heal after the watchdog
    // restarts the kernel.
    loop {
        if !subscription.is_current() {
            return Ok(());
        }

        let conn = match connection::get_or_connect(endpoint.clone(), timeout) {
            Ok(conn) => conn,
            Err(error) => {
                emit_status(&app, generation, "offline", Some(error), None);
                sleep_interruptible(subscription, backoff);
                backoff = next_reconnect_backoff(backoff);
                continue;
            }
        };
        // Register before snapshot queries so events arriving during resync
        // remain buffered for this consumer instead of being dropped.
        let mut receiver = conn.subscribe_events();
        let observer = FlowObservation::from_connection(subscription.binding.clone(), conn.clone());
        let mut traffic_binding = subscription.binding.clone();
        traffic_binding.timeout = traffic_binding.timeout.min(Duration::from_millis(500));
        let traffic_observer = FlowObservation::from_connection(traffic_binding, conn.clone());
        let snapshot = match resync_snapshot(&app, subscription, &observer) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if error.is_unavailable()
                    && should_retire_after_reconcile_failure(
                        conn.has_pending_requests(),
                        conn.received_within(ACTIVE_FLOW_RECONCILE_INTERVAL + timeout),
                    )
                {
                    conn.retire();
                }
                emit_status(&app, generation, "reconnecting", Some(error), None);
                sleep_interruptible(subscription, backoff);
                backoff = next_reconnect_backoff(backoff);
                continue;
            }
        };
        let mut runtime = snapshot.get("runtime").cloned().unwrap_or_default();
        backoff = MIN_RECONNECT_BACKOFF;
        emit_status(&app, generation, "subscribed", None, Some(snapshot));
        let mut next_active_flow_reconcile = Instant::now() + ACTIVE_FLOW_RECONCILE_INTERVAL;
        let mut traffic_recovery = crate::services::traffic_sampler::Recovery::new(Instant::now());
        let mut traffic_error = None;

        let mut closed = false;
        while subscription.is_current() {
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
                    if !subscription.is_current() {
                        return Ok(());
                    }
                    if connection::is_gap_event(&source_event) {
                        emit_status(
                            &app,
                            generation,
                            "reconnecting",
                            Some(AppError {
                                code: "event_gap",
                                message: "事件超出传输预算，正在重新读取权威快照".into(),
                                details: None,
                            }),
                            None,
                        );
                        receiver = conn.subscribe_events();
                        let snapshot = resync_snapshot(&app, subscription, &observer)?;
                        runtime = snapshot.get("runtime").cloned().unwrap_or_default();
                        emit_status(&app, generation, "subscribed", None, Some(snapshot));
                        continue;
                    }
                    if !event_matches_runtime(&source_event, &runtime) {
                        // A peer can replace its runtime without closing IPC.
                        // Acquire a new baseline before forwarding that instance.
                        closed = true;
                        break;
                    }
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
                            let sampled_at = event
                                .occurred_at_unix_ms
                                .unwrap_or_else(crate::services::common::now_unix_ms);
                            traffic_recovery.pushed(
                                Instant::now(),
                                sampled_at,
                                crate::services::common::now_unix_ms(),
                            );
                            crate::services::traffic_sampler::handle_stats_sample(
                                &app, stats, sampled_at, generation,
                            );
                        }
                        _ => {}
                    }
                    if filter.matches(&source_event, Some(&event.event_type)) {
                        emit_gui_event(
                            &app,
                            GuiEventPayload {
                                generation,
                                event: project_for_ui(event),
                            },
                        );
                    }
                    false
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    emit_status(
                        &app,
                        generation,
                        "reconnecting",
                        Some(AppError {
                            code: "event_gap",
                            message: "事件读取存在缺口，正在重新读取权威快照".into(),
                            details: None,
                        }),
                        None,
                    );
                    receiver = conn.subscribe_events();
                    // A lagged receiver has lost one or more flow deltas.
                    // Re-establish an authoritative baseline instead of
                    // silently leaving the live connection page stale.
                    match resync_snapshot(&app, subscription, &observer) {
                        Ok(snapshot) => {
                            runtime = snapshot.get("runtime").cloned().unwrap_or_default();
                            emit_status(&app, generation, "subscribed", None, Some(snapshot))
                        }
                        Err(error) => {
                            emit_status(&app, generation, "reconnecting", Some(error), None);
                            closed = true;
                            break;
                        }
                    }
                    next_active_flow_reconcile = Instant::now() + ACTIVE_FLOW_RECONCILE_INTERVAL;
                    false
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    closed = true;
                    break;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => true,
            };

            if !conn.has_pending_requests() && traffic_recovery.due(Instant::now()) {
                // A live transport and successful flow queries do not prove
                // that traffic pushes are still arriving. Recover only this
                // missing observation, without restarting the shared stream.
                let result = tauri::async_runtime::block_on(traffic_observer.traffic());
                if !subscription.is_current() {
                    return Ok(());
                }
                match result {
                    Ok((identity, stats)) => {
                        if !event_matches_runtime(&identity, &runtime) {
                            closed = true;
                            break;
                        }
                        crate::services::traffic_sampler::handle_stats_sample(
                            &app,
                            &stats,
                            crate::services::common::now_unix_ms(),
                            generation,
                        );
                        traffic_error = None;
                    }
                    Err(error) => {
                        if traffic_error.as_ref() != Some(&error.message) {
                            crate::services::file_logger::line(&format!(
                                "traffic recovery query failed: {}",
                                error.message
                            ));
                            traffic_error = Some(error.message);
                        }
                    }
                }
            }

            if Instant::now() >= next_active_flow_reconcile {
                // Flow pushes remain the low-latency path, but the active-flow
                // query is the authoritative state. Reconcile it periodically
                // so a dropped/quiet push stream cannot freeze the live page.
                match resync_active_connections(&observer) {
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

        if !subscription.is_current() {
            return Ok(());
        }
        if closed {
            emit_status(&app, generation, "reconnecting", None, None);
            sleep_interruptible(subscription, backoff);
            backoff = next_reconnect_backoff(backoff);
        }
    }
}

fn event_matches_runtime(event: &Value, runtime: &Value) -> bool {
    let instance = |value: &Value| {
        value
            .get("core_instance_id")
            .or_else(|| value.get("coreInstanceId"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match instance(event) {
        Some(id) => instance(runtime).as_deref() == Some(id.as_str()),
        // Older event envelopes lack identity; the pinned transport is then
        // the available isolation boundary, until the next baseline read.
        None => true,
    }
}

/// Preserve unknown type/reason/public facts, but never let its untyped raw
/// summary bypass the existing credential/configuration redaction boundary.
fn project_for_ui(mut event: GuiEvent) -> GuiEvent {
    if let GuiEventData::Unknown(payload) = &mut event.payload {
        payload.summary = payload
            .summary
            .as_ref()
            .map(crate::kernel::redaction::sensitive);
        event.source_event_type = crate::kernel::redaction::text(&event.source_event_type);
        event.event_id = event
            .event_id
            .as_deref()
            .map(crate::kernel::redaction::text);
        event.core_instance_id = event
            .core_instance_id
            .as_deref()
            .map(crate::kernel::redaction::text);
    }
    event
}

fn next_reconnect_backoff(current: Duration) -> Duration {
    (current * 2).min(MAX_RECONNECT_BACKOFF)
}

fn resync_snapshot(
    app: &AppHandle,
    subscription: &Subscription,
    observer: &FlowObservation,
) -> AppResult<Value> {
    let snapshot = tauri::async_runtime::block_on(observer.snapshot())?;
    if subscription.is_current() {
        if let Some(policies) = snapshot.get("policies").filter(|v| !v.is_null()) {
            let groups = crate::kernel::zero::parsing::parse_policy_groups(policies);
            crate::services::probe::reconcile_policy_snapshot(app, &groups);
        }
    }
    Ok(snapshot)
}

fn resync_active_connections(observer: &FlowObservation) -> AppResult<Vec<GuiConnection>> {
    tauri::async_runtime::block_on(observer.active(Some(GuiConnectionListOptions {
        limit: Some(500),
        inbound_tag: None,
        principal_key: None,
    })))
    .map(|connections| connections.items)
}

fn emit_connection_snapshot(app: &AppHandle, generation: u64, connections: Vec<GuiConnection>) {
    if !app
        .state::<AppState>()
        .observations()
        .is_current(generation)
    {
        return;
    }
    emit_gui_event(
        app,
        GuiEventPayload {
            generation,
            event: GuiEvent {
                event_type: "connection.snapshot".to_string(),
                source_event_type: "gui.activeFlowsReconcile".to_string(),
                event_id: None,
                sequence: None,
                sequence_exact: None,
                core_instance_id: None,
                occurred_at_unix_ms: Some(crate::services::common::now_unix_ms()),
                payload: GuiEventData::Connections(connections),
            },
        },
    );
}

/// Sleep for `total`, waking early if the subscription generation is
/// superseded — so `stop` takes effect promptly instead of waiting out the
/// full reconnect backoff.
fn sleep_interruptible(subscription: &Subscription, total: Duration) {
    let step = Duration::from_millis(200);
    let mut waited = Duration::ZERO;
    while waited < total {
        if !subscription.is_current() {
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
    if !app
        .state::<AppState>()
        .observations()
        .is_current(generation)
    {
        return;
    }
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
    use super::{event_matches_runtime, project_for_ui, should_retire_after_reconcile_failure};
    use serde_json::json;

    #[test]
    fn unknown_gui_event_secret_canaries_are_removed_before_ui_delivery_without_hiding_metadata() {
        let source = json!({"event_type":"future.public-event","event_id":"https://example.test/path?token=event-id-canary",
            "core_instance_id":"actual-current-instance","config_revision":1,"sequence":u64::MAX,
            "payload":{"config":{"password":"body-config-canary"},"credentials":"body-credentials-canary",
                "private_key":"body-private-key-canary","url":"https://example.test/?token=body-url-canary",
                "message":"request failed https://example.test/?token=message-url-canary","public_count":5}});
        let event = project_for_ui(crate::kernel::zero::events::normalize_event(&source));
        let serialized = serde_json::to_string(&event).unwrap();
        for canary in [
            "event-id-canary",
            "body-config-canary",
            "body-credentials-canary",
            "body-private-key-canary",
            "body-url-canary",
            "message-url-canary",
        ] {
            assert!(
                !serialized.contains(canary),
                "secret leaked before GUI event delivery"
            );
        }
        assert_eq!(event.source_event_type, "future.public-event");
        assert_eq!(
            event.core_instance_id.as_deref(),
            Some("actual-current-instance")
        );
        assert_eq!(
            event.sequence_exact.as_deref(),
            Some("18446744073709551615")
        );
        let crate::models::gui_core::GuiEventData::Unknown(payload) = event.payload else {
            panic!("unknown event must remain visible");
        };
        assert_eq!(
            payload.message.as_deref(),
            Some("unsupported zero event type")
        );
        assert_eq!(payload.summary.unwrap()["public_count"], 5);
    }

    #[test]
    fn event_identity_must_match_the_bound_runtime_when_present() {
        let runtime = json!({"core_instance_id":"current"});
        assert!(event_matches_runtime(
            &json!({"core_instance_id":"current"}),
            &runtime
        ));
        assert!(!event_matches_runtime(
            &json!({"core_instance_id":"old"}),
            &runtime
        ));
        assert!(event_matches_runtime(
            &json!({"coreInstanceId":"current"}),
            &runtime
        ));
        assert!(event_matches_runtime(
            &json!({"event_type":"legacy"}),
            &runtime
        ));
    }

    #[test]
    fn reconcile_timeout_never_retires_a_busy_or_recent_connection() {
        assert!(!should_retire_after_reconcile_failure(true, false));
        assert!(!should_retire_after_reconcile_failure(false, true));
        assert!(!should_retire_after_reconcile_failure(true, true));
        assert!(should_retire_after_reconcile_failure(false, false));
    }
}
