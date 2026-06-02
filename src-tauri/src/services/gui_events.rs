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
    emit_gui_event, emit_gui_event_status, GUI_EVENT_NAME, GUI_EVENT_STATUS_NAME,
};
use crate::models::{
    core::{CoreEndpoint, CoreIpcOptions},
    gui_core::{
        GuiConfigChangedEvent, GuiCoreHealth, GuiEvent, GuiEventData, GuiEventPayload,
        GuiEventStatus, GuiEventSubscription, GuiPolicyMember, GuiPolicyProbeCompletedEvent,
        GuiPolicySelectedEvent, GuiStackStatusEvent, GuiTunStatusEvent, GuiUnknownEvent,
        GuiWarningEvent,
    },
};
use crate::services::control_plane::{endpoint_from_options, timeout_from_options};
use crate::services::zero_adapter;

pub fn start(
    app: AppHandle,
    active_generation: Arc<AtomicU64>,
    generation: u64,
    events: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiEventSubscription> {
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
    emit_status(&app, generation, "subscribed", None);

    while active_generation.load(Ordering::SeqCst) == generation {
        let source_event = stream.read_next()?;
        let event = normalize_event(&source_event);
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

fn normalize_event(source: &Value) -> GuiEvent {
    let source_event_type = zero_adapter::string_at(source, &["event_type", "eventType", "type"])
        .unwrap_or_else(|| "unknown".to_string());
    let payload = source.get("payload").unwrap_or(source);

    GuiEvent {
        event_type: gui_event_type(&source_event_type).to_string(),
        source_event_type,
        event_id: zero_adapter::string_at(source, &["event_id", "eventId"]),
        sequence: zero_adapter::u64_at(source, &["sequence"]),
        occurred_at_unix_ms: zero_adapter::u64_at(
            source,
            &["occurred_at_unix_ms", "occurredAtUnixMs"],
        ),
        payload: normalize_payload(
            zero_adapter::string_at(source, &["event_type", "eventType", "type"])
                .as_deref()
                .unwrap_or("unknown"),
            payload,
        ),
    }
}

fn normalize_payload(source_event_type: &str, payload: &Value) -> GuiEventData {
    match source_event_type {
        "engine.started" => GuiEventData::CoreStatus(GuiCoreHealth {
            healthy: true,
            engine_version: zero_adapter::string_at(payload, &["version", "engine_version"]),
            started_at_unix_ms: zero_adapter::u64_at(
                payload,
                &["started_at_unix_ms", "startedAtUnixMs"],
            ),
        }),
        "engine.stopped" => GuiEventData::CoreStatus(GuiCoreHealth {
            healthy: false,
            engine_version: zero_adapter::string_at(payload, &["version", "engine_version"]),
            started_at_unix_ms: zero_adapter::u64_at(
                payload,
                &["started_at_unix_ms", "startedAtUnixMs"],
            ),
        }),
        "engine.warning" => GuiEventData::CoreWarning(GuiWarningEvent {
            code: zero_adapter::string_at(payload, &["code"]),
            message: zero_adapter::string_at(payload, &["message"])
                .unwrap_or_else(|| "core warning".to_string()),
        }),
        "config.changed" => GuiEventData::ConfigChanged(GuiConfigChangedEvent {
            changed_at_unix_ms: zero_adapter::u64_at(
                payload,
                &["changed_at_unix_ms", "changedAtUnixMs"],
            ),
        }),
        "flow.started" | "flow.updated" | "flow.completed" => {
            zero_adapter::parse_connection(payload)
                .map(GuiEventData::Connection)
                .unwrap_or_else(|| unknown_payload("invalid flow event payload", payload))
        }
        "policy.selected" => parse_policy_selected(payload)
            .map(GuiEventData::PolicySelected)
            .unwrap_or_else(|| unknown_payload("invalid policy.selected event payload", payload)),
        "policy.probe.completed" => parse_policy_probe_completed(payload)
            .map(GuiEventData::PolicyProbeCompleted)
            .unwrap_or_else(|| {
                unknown_payload("invalid policy.probe.completed event payload", payload)
            }),
        "stats.sampled" => GuiEventData::TrafficStats(zero_adapter::parse_stats(payload)),
        // v0.0.5+: TUN virtual network interface events
        "tun.started" => GuiEventData::TunStatus(GuiTunStatusEvent {
            state: "started".to_string(),
            interface_name: zero_adapter::string_at(payload, &["interface_name", "interfaceName", "tun_name", "tunName"]),
            address: zero_adapter::string_at(payload, &["address", "ip", "bind"]),
            message: None,
        }),
        "tun.stopped" => GuiEventData::TunStatus(GuiTunStatusEvent {
            state: "stopped".to_string(),
            interface_name: zero_adapter::string_at(payload, &["interface_name", "interfaceName", "tun_name", "tunName"]),
            address: None,
            message: zero_adapter::string_at(payload, &["message", "reason"]),
        }),
        "tun.error" => GuiEventData::TunStatus(GuiTunStatusEvent {
            state: "error".to_string(),
            interface_name: zero_adapter::string_at(payload, &["interface_name", "interfaceName", "tun_name", "tunName"]),
            address: None,
            message: zero_adapter::string_at(payload, &["message", "error", "reason"])
                .or_else(|| Some("TUN interface error".to_string())),
        }),
        // v0.0.5+: Network stack status (SystemStack / proxy stack)
        "stack.started" => GuiEventData::StackStatus(GuiStackStatusEvent {
            state: "started".to_string(),
            mode: zero_adapter::string_at(payload, &["mode", "stack_mode", "stackMode"]),
            message: None,
        }),
        "stack.stopped" => GuiEventData::StackStatus(GuiStackStatusEvent {
            state: "stopped".to_string(),
            mode: zero_adapter::string_at(payload, &["mode", "stack_mode", "stackMode"]),
            message: zero_adapter::string_at(payload, &["message", "reason"]),
        }),
        "stack.degraded" => GuiEventData::StackStatus(GuiStackStatusEvent {
            state: "degraded".to_string(),
            mode: zero_adapter::string_at(payload, &["mode", "stack_mode", "stackMode"]),
            message: zero_adapter::string_at(payload, &["message", "reason"])
                .or_else(|| Some("stack operating in degraded mode".to_string())),
        }),
        _ => unknown_payload("unsupported zero event type", payload),
    }
}

fn parse_policy_selected(payload: &Value) -> Option<GuiPolicySelectedEvent> {
    Some(GuiPolicySelectedEvent {
        policy_tag: zero_adapter::string_at(payload, &["policy_tag", "policyTag"])?,
        policy_kind: zero_adapter::string_at(payload, &["policy_kind", "policyKind"]),
        selected: zero_adapter::string_at(payload, &["selected"])?,
        previous: zero_adapter::string_at(payload, &["previous"]),
    })
}

fn parse_policy_probe_completed(payload: &Value) -> Option<GuiPolicyProbeCompletedEvent> {
    let selected = zero_adapter::string_at(payload, &["selected"]);
    let members = payload
        .get("members")
        .and_then(Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(|member| parse_policy_probe_member(member, selected.as_deref()))
                .collect()
        })
        .unwrap_or_default();

    Some(GuiPolicyProbeCompletedEvent {
        policy_tag: zero_adapter::string_at(payload, &["policy_tag", "policyTag"])?,
        selected,
        members,
    })
}

fn parse_policy_probe_member(payload: &Value, selected: Option<&str>) -> Option<GuiPolicyMember> {
    let tag = zero_adapter::string_at(payload, &["target_tag", "targetTag", "tag", "name"])?;
    Some(GuiPolicyMember {
        selected: selected.is_some_and(|selected| selected == tag),
        tag,
        kind: zero_adapter::string_at(payload, &["kind", "type", "protocol"]),
        alive: payload
            .get("healthy")
            .and_then(Value::as_bool)
            .or_else(|| payload.get("alive").and_then(Value::as_bool)),
        delay_ms: zero_adapter::u64_at(
            payload,
            &["latency_ms", "latencyMs", "delay_ms", "delayMs"],
        ),
    })
}

fn unknown_payload(message: &'static str, payload: &Value) -> GuiEventData {
    GuiEventData::Unknown(GuiUnknownEvent {
        message: Some(message.to_string()),
        summary: Some(payload.clone()),
    })
}

fn gui_event_type(source_event_type: &str) -> &'static str {
    match source_event_type {
        "engine.started" | "engine.stopped" => "core.statusChanged",
        "engine.warning" => "core.warning",
        "config.changed" => "core.configChanged",
        "flow.started" => "connection.started",
        "flow.updated" => "connection.updated",
        "flow.completed" => "connection.closed",
        "policy.selected" => "policy.selected",
        "policy.probe.completed" => "policy.probeCompleted",
        "stats.sampled" => "traffic.sampled",
        // v0.0.5+: TUN virtual network interface
        "tun.started" | "tun.stopped" => "tun.statusChanged",
        "tun.error" => "tun.error",
        // v0.0.5+: Network stack (SystemStack / proxy)
        "stack.started" | "stack.stopped" | "stack.degraded" => "stack.statusChanged",
        _ => "core.unknownEvent",
    }
}
