//! Zero kernel event normalization.
//!
//! Maps raw `ApiEvent` JSON from the kernel into typed `GuiEvent`
//! payloads. The IPC transport delivers opaque JSON; this module
//! adds the kernel-specific interpretation layer.

use serde_json::Value;

use crate::models::gui_core::{
    GuiConfigChangedEvent, GuiCoreHealth, GuiEvent, GuiEventData,
    GuiPolicyMember, GuiPolicyProbeCompletedEvent, GuiPolicySelectedEvent,
    GuiStackStatusEvent, GuiTunStatusEvent, GuiUnknownEvent, GuiWarningEvent,
};

use super::parsing::{normalize_version, parse_connection, parse_stats, string_at, u64_at};

/// Normalize a raw kernel event into a typed `GuiEvent`.
pub fn normalize_event(source: &Value) -> GuiEvent {
    let source_event_type = string_at(source, &["event_type", "eventType", "type"])
        .unwrap_or_else(|| "unknown".to_string());
    let payload = source.get("payload").unwrap_or(source);

    GuiEvent {
        event_type: gui_event_type(&source_event_type).to_string(),
        source_event_type,
        event_id: string_at(source, &["event_id", "eventId"]),
        sequence: u64_at(source, &["sequence"]),
        occurred_at_unix_ms: u64_at(source, &["occurred_at_unix_ms", "occurredAtUnixMs"]),
        payload: normalize_payload(
            string_at(source, &["event_type", "eventType", "type"])
                .as_deref()
                .unwrap_or("unknown"),
            payload,
        ),
    }
}

/// Map a kernel event type to a GUI event type.
pub fn gui_event_type(source_event_type: &str) -> &'static str {
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
        // TUN virtual network interface
        "tun.started" | "tun.stopped" => "tun.statusChanged",
        "tun.error" => "tun.error",
        // Network stack (SystemStack / proxy)
        "stack.started" | "stack.stopped" | "stack.degraded" => "stack.statusChanged",
        _ => "core.unknownEvent",
    }
}

fn normalize_payload(source_event_type: &str, payload: &Value) -> GuiEventData {
    match source_event_type {
        "engine.started" => GuiEventData::CoreStatus(GuiCoreHealth {
            healthy: true,
            engine_version: normalize_version(string_at(
                payload,
                &["build_id", "version", "engine_version"],
            )),
            started_at_unix_ms: u64_at(
                payload,
                &["started_at_unix_ms", "startedAtUnixMs"],
            ),
        }),
        "engine.stopped" => GuiEventData::CoreStatus(GuiCoreHealth {
            healthy: false,
            engine_version: normalize_version(string_at(
                payload,
                &["build_id", "version", "engine_version"],
            )),
            started_at_unix_ms: u64_at(
                payload,
                &["started_at_unix_ms", "startedAtUnixMs"],
            ),
        }),
        "engine.warning" => GuiEventData::CoreWarning(GuiWarningEvent {
            code: string_at(payload, &["code"]),
            message: string_at(payload, &["message"])
                .unwrap_or_else(|| "core warning".to_string()),
        }),
        "config.changed" => GuiEventData::ConfigChanged(GuiConfigChangedEvent {
            changed_at_unix_ms: u64_at(payload, &["changed_at_unix_ms", "changedAtUnixMs"]),
        }),
        "flow.started" | "flow.updated" | "flow.completed" => {
            parse_connection(payload)
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
        "stats.sampled" => GuiEventData::TrafficStats(parse_stats(payload)),
        // TUN virtual network interface events
        "tun.started" => GuiEventData::TunStatus(GuiTunStatusEvent {
            state: "started".to_string(),
            interface_name: string_at(
                payload,
                &["interface_name", "interfaceName", "name", "tun_name", "tunName"],
            ),
            address: string_at(payload, &["address", "addr", "ip", "bind"]),
            message: None,
        }),
        "tun.stopped" => GuiEventData::TunStatus(GuiTunStatusEvent {
            state: "stopped".to_string(),
            interface_name: string_at(
                payload,
                &["interface_name", "interfaceName", "name", "tun_name", "tunName"],
            ),
            address: None,
            message: string_at(payload, &["message", "reason"]),
        }),
        "tun.error" => GuiEventData::TunStatus(GuiTunStatusEvent {
            state: "error".to_string(),
            interface_name: string_at(
                payload,
                &["interface_name", "interfaceName", "name", "tun_name", "tunName"],
            ),
            address: None,
            message: string_at(payload, &["message", "error", "reason"])
                .or_else(|| Some("TUN interface error".to_string())),
        }),
        // Network stack status (SystemStack / proxy stack)
        "stack.started" => GuiEventData::StackStatus(GuiStackStatusEvent {
            state: "started".to_string(),
            mode: string_at(payload, &["mode", "stack_mode", "stackMode"]),
            message: None,
        }),
        "stack.stopped" => GuiEventData::StackStatus(GuiStackStatusEvent {
            state: "stopped".to_string(),
            mode: string_at(payload, &["mode", "stack_mode", "stackMode"]),
            message: string_at(payload, &["message", "reason"]),
        }),
        "stack.degraded" => GuiEventData::StackStatus(GuiStackStatusEvent {
            state: "degraded".to_string(),
            mode: string_at(payload, &["mode", "stack_mode", "stackMode"]),
            message: string_at(payload, &["message", "reason"])
                .or_else(|| Some("stack operating in degraded mode".to_string())),
        }),
        _ => unknown_payload("unsupported zero event type", payload),
    }
}

fn parse_policy_selected(payload: &Value) -> Option<GuiPolicySelectedEvent> {
    Some(GuiPolicySelectedEvent {
        policy_tag: string_at(payload, &["policy_tag", "policyTag"])?,
        policy_kind: string_at(payload, &["policy_kind", "policyKind"]),
        selected: string_at(payload, &["selected"])?,
        previous: string_at(payload, &["previous"]),
    })
}

fn parse_policy_probe_completed(payload: &Value) -> Option<GuiPolicyProbeCompletedEvent> {
    let selected = string_at(payload, &["selected"]);
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
        policy_tag: string_at(payload, &["policy_tag", "policyTag"])?,
        selected,
        members,
    })
}

fn parse_policy_probe_member(payload: &Value, selected: Option<&str>) -> Option<GuiPolicyMember> {
    let tag = string_at(payload, &["target_tag", "targetTag", "tag", "name"])?;
    Some(GuiPolicyMember {
        selected: selected.is_some_and(|selected| selected == tag),
        tag,
        kind: string_at(payload, &["kind", "type", "protocol"]),
        alive: payload
            .get("healthy")
            .and_then(Value::as_bool)
            .or_else(|| payload.get("alive").and_then(Value::as_bool)),
        delay_ms: u64_at(
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
