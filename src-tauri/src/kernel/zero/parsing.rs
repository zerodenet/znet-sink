//! Pure JSON parsing helpers for Zero kernel responses.
//!
//! All functions are pure (no I/O, no state). They accept a `&Value`
//! and return parsed GUI model types. This separation makes the
//! parsing logic trivially testable without kernel IPC.

use serde_json::Value;

use crate::errors::{AppError, AppResult};
use crate::models::gui_core::{
    GuiApiContractVersions, GuiCapabilityEndpoint, GuiCapabilityState, GuiConfigImpactItem,
    GuiConfigPlanApplyResult, GuiConnection, GuiConnectionAddressFamilyFallback,
    GuiConnectionCloseResult, GuiConnectionEgressContext, GuiConnectionList,
    GuiConnectionNetworkContext, GuiConnectionNetworkInterface, GuiConnectionRouteLookup,
    GuiConnectionSocketBinding, GuiContractVersionRange, GuiCoreHealth, GuiFakeIpClearResult,
    GuiFeatureStatus, GuiPolicyGroup, GuiPolicyMember, GuiPolicySelectionResult,
    GuiProtocolCapability, GuiTargetProbeResult, GuiTrafficStats, GuiZeroCapabilities,
};

// ── Response envelope helpers ───────────────────────────────────────

/// Unwrap a `CoreCallResult`'s optional response/error into a raw `Value`.
pub fn unwrap_call_result(response: Option<Value>, error: Option<AppError>) -> AppResult<Value> {
    if let Some(error) = error {
        return Err(error);
    }

    let response = response.ok_or_else(|| AppError::internal("core returned no response"))?;
    unwrap_core_envelope(response)
}

/// Strip the `{"ok":bool, "result":...}` envelope.
///
/// If `ok` is `false`, returns an error. Otherwise returns the `result`
/// field (or the raw object if no envelope is detected).
pub fn unwrap_core_envelope(response: Value) -> AppResult<Value> {
    let Some(object) = response.as_object() else {
        return Ok(response);
    };

    if let Some(false) = object.get("ok").and_then(Value::as_bool) {
        return Err(AppError::core_response(Value::Object(object.clone())));
    }
    if object.contains_key("ok") {
        return Ok(object.get("result").cloned().unwrap_or(Value::Null));
    }

    Ok(Value::Object(object.clone()))
}

/// Strip the `{"ok":bool, "result":{variant_key:...}}` envelope AND
/// unwrap the externally-tagged QueryResponse variant.
///
/// IPC Query responses use `result.health`, `result.active_flows`, etc.
/// This helper strips both the envelope and the variant wrapper.
/// Falls back to the raw `result` when the variant key is not present
/// (backward-compatible with older kernels or flat shapes).
///
/// Production code uses the split approach (unwrap_core_envelope + local
/// variant unwrapping in queries.rs). This combined helper is kept for
/// test ergonomics.
#[allow(dead_code)]
pub fn unwrap_query_variant(response: Value, variant: &str) -> AppResult<Value> {
    let inner = unwrap_core_envelope(response)?;

    // Try to unwrap the variant key: result.{variant}
    if let Some(obj) = inner.as_object() {
        if let Some(variant_data) = obj.get(variant) {
            return Ok(variant_data.clone());
        }
    }

    // Fallback: result is already the inner data (flat shape or old kernel)
    Ok(inner)
}

// ── Parsers ─────────────────────────────────────────────────────────

pub fn parse_health(value: &Value) -> GuiCoreHealth {
    GuiCoreHealth {
        healthy: bool_at(value, &["healthy"]).unwrap_or(true),
        engine_version: normalize_version(string_at(
            value,
            &[
                "engine_build_id",
                "engine_version",
                "engineVersion",
                "version",
            ],
        )),
        started_at_unix_ms: u64_at(
            value,
            &["started_at_unix_ms", "startedAtUnixMs", "started_at"],
        ),
    }
}

pub fn parse_stats(value: &Value) -> GuiTrafficStats {
    let stats = nested_value(value, &["stats"]).unwrap_or(value);
    GuiTrafficStats {
        active_sessions: u64_at(stats, &["active_sessions", "activeSessions"]).unwrap_or(0),
        total_started: u64_at(stats, &["total_started", "totalStarted"]).unwrap_or(0),
        completed_sessions: u64_at(stats, &["completed_sessions", "completedSessions"])
            .unwrap_or(0),
        failed_sessions: u64_at(stats, &["failed_sessions", "failedSessions"]).unwrap_or(0),
        blocked_sessions: u64_at(stats, &["blocked_sessions", "blockedSessions"]).unwrap_or(0),
        direct_sessions: u64_at(stats, &["direct_sessions", "directSessions"]).unwrap_or(0),
        chained_sessions: u64_at(stats, &["chained_sessions", "chainedSessions"]).unwrap_or(0),
        bytes_up: u64_at(stats, &["bytes_up", "bytesUp", "upload", "tx"]).unwrap_or(0),
        bytes_down: u64_at(stats, &["bytes_down", "bytesDown", "download", "rx"]).unwrap_or(0),
    }
}

pub fn parse_capabilities(value: &Value, error: Option<String>) -> GuiZeroCapabilities {
    GuiZeroCapabilities {
        available: error.is_none(),
        api_version: string_at(value, &["api_id", "api_version", "apiVersion"]),
        schema_version: string_at(value, &["schema_id", "schema_version", "schemaVersion"]),
        contracts: parse_contract_versions(value),
        error_codes: string_array_at(value, &["error_codes", "errorCodes"]),
        global_limitations: string_array_at(value, &["global_limitations", "globalLimitations"]),
        features: string_array_at(value, &["features"]),
        permissions: string_array_at(value, &["permissions"]),
        adapters: endpoint_array_at(value, "adapters"),
        sinks: endpoint_array_at(value, "sinks"),
        protocols: protocol_array_at(value, "protocols"),
        build_features: string_array_at(value, &["build_features", "buildFeatures"]),
        error,
    }
}

fn parse_contract_versions(value: &Value) -> Option<GuiApiContractVersions> {
    let contracts = value.get("contracts")?;
    Some(GuiApiContractVersions {
        capabilities: parse_contract_version_range(contracts, &["capabilities"]),
        control_api: parse_contract_version_range(contracts, &["control_api", "controlApi"]),
        config_schema: parse_contract_version_range(contracts, &["config_schema", "configSchema"]),
        error_codes: parse_contract_version_range(contracts, &["error_codes", "errorCodes"]),
    })
}

fn parse_contract_version_range(value: &Value, keys: &[&str]) -> GuiContractVersionRange {
    let range = keys.iter().find_map(|key| value.get(*key));
    GuiContractVersionRange {
        current: range
            .and_then(|range| u64_at(range, &["current"]))
            .unwrap_or(0),
        minimum_supported: range
            .and_then(|range| u64_at(range, &["minimum_supported", "minimumSupported"]))
            .unwrap_or(0),
    }
}

pub fn parse_policy_groups(value: &Value) -> Vec<GuiPolicyGroup> {
    values_from_container(
        value,
        &[
            "policies",
            "policy_groups",
            "policyGroups",
            "groups",
            "outbounds",
            "items",
        ],
    )
    .into_iter()
    .filter_map(parse_policy_group)
    .collect()
}

fn parse_policy_group(value: Value) -> Option<GuiPolicyGroup> {
    let tag = string_at(&value, &["policy_tag", "policyTag", "tag", "name", "id"])?;
    let selected = string_at(&value, &["selected", "current", "now", "target"]);
    let members = parse_policy_members(&value, selected.as_deref());

    Some(GuiPolicyGroup {
        name: tag,
        kind: string_at(&value, &["policy_kind", "policyKind", "kind", "type"])
            .unwrap_or_else(|| "unknown".to_string()),
        selected,
        outbounds: members,
        available: bool_at(&value, &["available", "healthy"]).unwrap_or(true),
        reason: string_at(&value, &["reason", "error", "message"]),
    })
}

fn parse_policy_members(value: &Value, selected: Option<&str>) -> Vec<GuiPolicyMember> {
    values_from_container(
        value,
        &[
            "url_test_members",
            "members",
            "targets",
            "children",
            "proxies",
            "outbounds",
            "items",
        ],
    )
    .into_iter()
    .filter_map(|member| parse_policy_member(member, selected))
    .collect()
}

fn parse_policy_member(value: Value, selected: Option<&str>) -> Option<GuiPolicyMember> {
    let tag = match &value {
        Value::String(tag) => tag.clone(),
        other => string_at(other, &["member_tag", "tag", "name", "id", "target"])?,
    };
    let source = if value.is_object() { Some(value) } else { None };
    let kind = source
        .as_ref()
        .and_then(|value| string_at(value, &["kind", "type", "protocol"]));
    let alive = source
        .as_ref()
        .and_then(|value| bool_at(value, &["alive", "healthy", "available"]));
    let delay_ms = source
        .as_ref()
        .and_then(|value| u64_at(value, &["delay_ms", "delayMs", "latency", "latency_ms"]));
    let last_checked_unix_ms = source.as_ref().and_then(|value| {
        u64_at(
            value,
            &[
                "last_checked_unix_ms",
                "lastCheckedUnixMs",
                "checked_at_unix_ms",
            ],
        )
    });
    let last_error = source
        .as_ref()
        .and_then(|value| string_at(value, &["last_error", "lastError", "error"]));

    Some(GuiPolicyMember {
        selected: selected.is_some_and(|selected| selected == tag),
        tag,
        kind,
        alive,
        delay_ms,
        last_checked_unix_ms,
        last_error,
    })
}

pub fn parse_policy_selection(
    value: &Value,
    policy_tag: String,
    target_tag: String,
) -> GuiPolicySelectionResult {
    let result = nested_value(value, &["result"]).unwrap_or(value);
    GuiPolicySelectionResult {
        policy_tag: string_at(result, &["policy_tag", "policyTag"]).unwrap_or(policy_tag),
        target_tag,
        selected: string_at(result, &["selected", "target_tag", "targetTag"]),
        accepted: bool_at(value, &["accepted"]).unwrap_or(true),
        message: string_at(result, &["message"]),
    }
}

pub fn parse_target_probe(value: &Value, target_tag: String) -> GuiTargetProbeResult {
    let result = nested_value(value, &["result"]).unwrap_or(value);
    GuiTargetProbeResult {
        target_tag: string_at(result, &["target_tag", "targetTag"]).unwrap_or(target_tag),
        reachable: bool_at(result, &["reachable", "healthy", "available"]).unwrap_or(false),
        latency_ms: u64_at(
            result,
            &["latency_ms", "latencyMs", "delay_ms", "delayMs", "latency"],
        ),
        server: string_at(result, &["server", "address", "host"]),
        port: u64_at(result, &["port"]),
        message: string_at(result, &["message", "reason", "error"]),
    }
}

pub fn parse_connection_list(value: &Value, limit: u32) -> GuiConnectionList {
    let items = values_from_container(value, &["flows", "connections", "items", "data", "active"])
        .into_iter()
        .filter_map(|value| parse_connection(&value))
        .collect::<Vec<_>>();

    GuiConnectionList {
        total: u64_at(value, &["total", "count"]),
        items,
        limit,
    }
}

pub fn parse_connection(value: &Value) -> Option<GuiConnection> {
    let value = nested_value(value, &["record"]).unwrap_or(value);
    let flow_id = string_at(
        value,
        &["flow_id", "flowId", "id", "connection_id", "connectionId"],
    )?;
    let target = nested_value(value, &["target"]);
    let host = target
        .and_then(|target| string_at(target, &["host", "address", "value"]))
        .or_else(|| string_at(value, &["host", "destination", "dest", "remote", "address"]));
    let port = target
        .and_then(|target| u64_at(target, &["port"]))
        .or_else(|| {
            u64_at(
                value,
                &["port", "dest_port", "destPort", "remote_port", "remotePort"],
            )
        });
    let destination = endpoint_display(host.as_deref(), port).unwrap_or_else(|| "-".to_string());
    let source_info = nested_value(value, &["source"]);
    let source_ip = source_info
        .and_then(|source| string_at(source, &["ip", "address", "host"]))
        .or_else(|| string_at(value, &["source_ip", "sourceIp", "client_ip", "clientIp"]));
    let source_port = source_info
        .and_then(|source| u64_at(source, &["port"]))
        .or_else(|| {
            u64_at(
                value,
                &["source_port", "sourcePort", "client_port", "clientPort"],
            )
        });
    let source = endpoint_display(source_ip.as_deref(), source_port)
        .or_else(|| string_at(value, &["source", "client", "local"]));
    let traffic = nested_value(value, &["traffic"]).unwrap_or(value);
    let timing = nested_value(value, &["timing"]).unwrap_or(value);
    let throughput = nested_value(value, &["throughput"]).unwrap_or(value);
    let inbound = nested_value(value, &["inbound"]);
    let path = nested_value(value, &["path"]);
    let outbound = path
        .and_then(|path| nested_value(path, &["outbound"]))
        .or_else(|| nested_value(value, &["outbound"]));
    let route = nested_value(value, &["route"]);
    let result = nested_value(value, &["result"]);
    let failure = result.and_then(|result| nested_value(result, &["failure"]));
    let network_context = path
        .and_then(|path| path.get("network").or_else(|| path.get("networkContext")))
        .and_then(parse_connection_network_context);
    let remote = path
        .and_then(|path| nested_value(path, &["remote"]))
        .or_else(|| failure.and_then(|failure| nested_value(failure, &["remote"])));
    let remote_host = remote.and_then(|remote| string_at(remote, &["host", "address", "ip"]));
    let remote_port = remote.and_then(|remote| u64_at(remote, &["port"]));
    let selection_chain = route
        .and_then(|route| {
            route
                .get("selection_chain")
                .or_else(|| route.get("selectionChain"))
        })
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let relay_chain = path
        .and_then(|path| path.get("relay_chain").or_else(|| path.get("relayChain")))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| string_at(item, &["tag", "name"]))
                })
                .collect()
        })
        .unwrap_or_default();
    let matched_rule = route.and_then(|route| {
        route
            .get("matched_rule")
            .or_else(|| route.get("matchedRule"))
    });

    Some(GuiConnection {
        flow_id,
        revision: u64_at(value, &["revision"]),
        state: string_at(value, &["state"]),
        network: string_at(value, &["network", "protocol", "type"])
            .unwrap_or_else(|| "tcp".to_string()),
        source,
        source_ip,
        source_port,
        process_id: source_info
            .and_then(|source| u64_at(source, &["process_id", "processId", "pid"])),
        process_name: source_info
            .and_then(|source| string_at(source, &["process_name", "processName", "process"])),
        process_path: source_info
            .and_then(|source| string_at(source, &["process_path", "processPath"])),
        destination,
        target_host: host,
        target_ip: target.and_then(|target| {
            string_at(target, &["resolved_ip", "resolvedIp"]).or_else(|| {
                let family = string_at(target, &["family"])?;
                (family == "ipv4" || family == "ipv6")
                    .then(|| string_at(target, &["value"]))
                    .flatten()
            })
        }),
        target_port: port,
        original_ip: target.and_then(|target| string_at(target, &["original_ip", "originalIp"])),
        host_source: target.and_then(|target| string_at(target, &["host_source", "hostSource"])),
        fake_ip_reverse_status: target.and_then(|target| {
            string_at(target, &["fake_ip_reverse_status", "fakeIpReverseStatus"])
        }),
        sniffed_host: target.and_then(|target| string_at(target, &["sniffed_host", "sniffedHost"])),
        inbound_tag: inbound
            .and_then(|inbound| string_at(inbound, &["tag", "protocol"]))
            .or_else(|| string_at(value, &["inbound_tag", "inboundTag"])),
        inbound_protocol: inbound.and_then(|inbound| string_at(inbound, &["protocol", "type"])),
        outbound_tag: outbound
            .and_then(|outbound| string_at(outbound, &["tag", "protocol"]))
            .or_else(|| string_at(value, &["outbound_tag", "outboundTag"])),
        outbound_protocol: outbound.and_then(|outbound| string_at(outbound, &["protocol", "type"])),
        remote_destination: endpoint_display(remote_host.as_deref(), remote_port).or_else(|| {
            network_context
                .as_ref()
                .and_then(|network| network.remote_address.clone())
        }),
        network_context,
        policy_tag: nested_value(value, &["policy"])
            .and_then(|policy| string_at(policy, &["tag", "policy_tag", "policyTag"]))
            .or_else(|| route.and_then(|route| string_at(route, &["target"])))
            .or_else(|| string_at(value, &["policy_tag", "policyTag"])),
        route_mode: route
            .and_then(|route| string_at(route, &["mode"]))
            .or_else(|| string_at(value, &["route_mode", "routeMode", "mode"])),
        route_action: route.and_then(|route| string_at(route, &["action"])),
        matched_rule_index: matched_rule.and_then(|rule| u64_at(rule, &["index"])),
        matched_rule: matched_rule.and_then(|rule| string_at(rule, &["condition", "rule"])),
        selection_chain,
        relay_chain,
        outcome: result
            .and_then(|result| string_at(result, &["outcome"]))
            .or_else(|| string_at(value, &["outcome", "status"])),
        close_reason: result
            .and_then(|result| string_at(result, &["close_reason", "closeReason"]))
            .or_else(|| string_at(value, &["close_reason", "closeReason"])),
        failure_stage: failure.and_then(|failure| string_at(failure, &["stage"])),
        failure_code: failure.and_then(|failure| string_at(failure, &["code"])),
        failure_message: failure.and_then(|failure| string_at(failure, &["message", "error"])),
        bytes_up: u64_at(traffic, &["bytes_up", "bytesUp", "tx"]).unwrap_or(0),
        bytes_down: u64_at(traffic, &["bytes_down", "bytesDown", "rx"]).unwrap_or(0),
        inbound_rx_bytes: u64_at(traffic, &["inbound_rx_bytes", "inboundRxBytes"]),
        inbound_tx_bytes: u64_at(traffic, &["inbound_tx_bytes", "inboundTxBytes"]),
        outbound_rx_bytes: u64_at(traffic, &["outbound_rx_bytes", "outboundRxBytes"]),
        outbound_tx_bytes: u64_at(traffic, &["outbound_tx_bytes", "outboundTxBytes"]),
        throughput_up_bps: u64_at(
            throughput,
            &[
                "upload_bps",
                "uploadBps",
                "throughput_up_bps",
                "throughputUpBps",
            ],
        ),
        throughput_down_bps: u64_at(
            throughput,
            &[
                "download_bps",
                "downloadBps",
                "throughput_down_bps",
                "throughputDownBps",
            ],
        ),
        started_at_unix_ms: u64_at(
            timing,
            &["started_at_unix_ms", "startedAtUnixMs", "started_at"],
        ),
        last_activity_at_unix_ms: u64_at(
            timing,
            &["last_activity_at_unix_ms", "lastActivityAtUnixMs"],
        ),
        ended_at_unix_ms: u64_at(
            timing,
            &[
                "ended_at_unix_ms",
                "endedAtUnixMs",
                "finished_at_unix_ms",
                "finishedAtUnixMs",
            ],
        ),
        updated_at_unix_ms: u64_at(
            throughput,
            &[
                "sampled_at_unix_ms",
                "sampledAtUnixMs",
                "snapshot_at_unix_ms",
                "updatedAtUnixMs",
                "updated_at",
            ],
        ),
        duration_ms: u64_at(timing, &["duration_ms", "durationMs"]),
    })
}

fn parse_connection_network_context(value: &Value) -> Option<GuiConnectionNetworkContext> {
    value.as_object()?;
    let resolved_candidates = value
        .get("resolved_candidates")
        .or_else(|| value.get("resolvedCandidates"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_network_address).collect())
        .unwrap_or_default();
    let selected_interface = value
        .get("selected_interface")
        .or_else(|| value.get("selectedInterface"))
        .and_then(parse_network_interface);
    let egress = value.get("egress").and_then(|egress| {
        egress.as_object()?;
        Some(GuiConnectionEgressContext {
            generation: u64_at(egress, &["generation"]),
            address_family: string_at(egress, &["address_family", "addressFamily"]),
            tun_active: bool_at(egress, &["tun_active", "tunActive"]),
            configured_interface: egress
                .get("configured_interface")
                .or_else(|| egress.get("configuredInterface"))
                .and_then(parse_network_interface),
            unavailable_reason: string_at(egress, &["unavailable_reason", "unavailableReason"]),
        })
    });
    let route_lookup = value
        .get("route_lookup")
        .or_else(|| value.get("routeLookup"))
        .and_then(|lookup| {
            lookup.as_object()?;
            Some(GuiConnectionRouteLookup {
                status: string_at(lookup, &["status"]),
                source_address: string_at(lookup, &["source_address", "sourceAddress"]),
                error: string_at(lookup, &["error"]),
            })
        });
    let socket_binding = value
        .get("socket_binding")
        .or_else(|| value.get("socketBinding"))
        .and_then(|binding| {
            binding.as_object()?;
            Some(GuiConnectionSocketBinding {
                mode: string_at(binding, &["mode"]),
                reason: string_at(binding, &["reason"]),
                interface_bound: bool_at(binding, &["interface_bound", "interfaceBound"]),
            })
        });
    let address_family_fallback = value
        .get("address_family_fallback")
        .or_else(|| value.get("addressFamilyFallback"))
        .and_then(|fallback| {
            fallback.as_object()?;
            let fallback = GuiConnectionAddressFamilyFallback {
                from: string_at(fallback, &["from"]),
                to: string_at(fallback, &["to"]),
                reason: string_at(fallback, &["reason"]),
                trigger_egress_generation: u64_at(
                    fallback,
                    &["trigger_egress_generation", "triggerEgressGeneration"],
                ),
                unavailable_reason: string_at(
                    fallback,
                    &["unavailable_reason", "unavailableReason"],
                ),
            };
            let populated = fallback.from.is_some()
                || fallback.to.is_some()
                || fallback.reason.is_some()
                || fallback.trigger_egress_generation.is_some()
                || fallback.unavailable_reason.is_some();
            populated.then_some(fallback)
        });

    let context = GuiConnectionNetworkContext {
        local_address: value
            .get("local_address")
            .or_else(|| value.get("localAddress"))
            .and_then(parse_network_address),
        remote_address: value
            .get("remote_address")
            .or_else(|| value.get("remoteAddress"))
            .and_then(parse_network_address),
        resolved_candidates,
        address_family_policy: string_at(value, &["address_family_policy", "addressFamilyPolicy"]),
        address_family_fallback,
        selected_interface,
        egress,
        route_lookup,
        socket_binding,
        connect_stage: string_at(value, &["connect_stage", "connectStage"]),
    };

    let populated = context.local_address.is_some()
        || context.remote_address.is_some()
        || !context.resolved_candidates.is_empty()
        || context.address_family_policy.is_some()
        || context.address_family_fallback.is_some()
        || context.selected_interface.is_some()
        || context.egress.is_some()
        || context.route_lookup.is_some()
        || context.socket_binding.is_some()
        || context.connect_stage.is_some();
    populated.then_some(context)
}

fn parse_network_interface(value: &Value) -> Option<GuiConnectionNetworkInterface> {
    Some(GuiConnectionNetworkInterface {
        name: string_at(value, &["name"])?,
        index: u64_at(value, &["index"]),
    })
}

fn parse_network_address(value: &Value) -> Option<String> {
    let host = string_at(value, &["host", "address", "ip", "value"]);
    let port = u64_at(value, &["port"]);
    endpoint_display(host.as_deref(), port)
}

fn endpoint_display(host: Option<&str>, port: Option<u64>) -> Option<String> {
    match (host, port) {
        (Some(host), Some(port)) if host.contains(':') => Some(format!("[{host}]:{port}")),
        (Some(host), Some(port)) => Some(format!("{host}:{port}")),
        (Some(host), None) => Some(host.to_owned()),
        (None, Some(port)) => Some(port.to_string()),
        (None, None) => None,
    }
}

pub fn parse_connection_close(value: &Value, flow_id: String) -> GuiConnectionCloseResult {
    GuiConnectionCloseResult {
        flow_id: string_at(value, &["flow_id", "flowId"]).unwrap_or(flow_id),
        closed: bool_at(value, &["closed"]).unwrap_or(true),
        message: string_at(value, &["message"]),
    }
}

pub fn parse_fake_ip_clear(value: &Value) -> GuiFakeIpClearResult {
    let result = nested_value(value, &["result"]).unwrap_or(value);
    GuiFakeIpClearResult {
        core_instance_id: string_at(result, &["core_instance_id", "coreInstanceId"]),
        config_revision: u64_at(result, &["config_revision", "configRevision"]),
        enabled: bool_at(result, &["enabled"]).unwrap_or(false),
        scope: string_at(result, &["scope"]).unwrap_or_else(|| "all".to_string()),
        domain: string_at(result, &["domain"]),
        ip: string_at(result, &["ip"]),
        removed_mappings: u64_at(result, &["removed_mappings", "removedMappings"]).unwrap_or(0),
        removed_addresses: u64_at(result, &["removed_addresses", "removedAddresses"]).unwrap_or(0),
        live_mappings: u64_at(result, &["live_mappings", "liveMappings"]).unwrap_or(0),
    }
}

pub fn parse_feature_runtime_status(
    key: &str,
    value: &Value,
    fallback: Option<&GuiFeatureStatus>,
) -> GuiFeatureStatus {
    let status = nested_value(value, &["result"])
        .or_else(|| nested_value(value, &["status"]))
        .or_else(|| nested_value(value, &[key]))
        .unwrap_or(value);
    let enabled = bool_at(status, &["running", "enabled", "active"])
        .or_else(|| {
            string_at(status, &["state", "status"]).map(|state| {
                matches!(
                    state.to_ascii_lowercase().as_str(),
                    "running" | "started" | "active" | "enabled"
                )
            })
        })
        .unwrap_or(false);
    let state = string_at(status, &["state", "status"])
        .unwrap_or_else(|| if enabled { "running" } else { "stopped" }.to_string());
    let reason = string_at(status, &["reason", "message", "error"])
        .or_else(|| fallback.and_then(|fallback| fallback.reason.clone()));

    GuiFeatureStatus {
        key: key.to_string(),
        supported: fallback.map(|fallback| fallback.supported).unwrap_or(true),
        enabled,
        state,
        reason,
    }
}

/// Parse `config.plan_apply` response into a structured impact analysis.
///
/// Expected kernel response shape:
/// ```json
/// {
///   "valid": true,
///   "hot_reload": [{ "section": "outbounds", "tags": [...], "detail": "..." }],
///   "requires_restart": [{ "section": "listeners", "tags": [...], "detail": "..." }],
///   "warnings": ["..."],
///   "errors": ["..."]
/// }
/// ```
///
/// The parser is tolerant — missing fields default to empty; unknown keys
/// are ignored. This lets the kernel evolve the response format without
/// breaking older GUI builds.
pub fn parse_plan_apply_result(value: &Value) -> GuiConfigPlanApplyResult {
    let result = nested_value(value, &["result"]).unwrap_or(value);

    GuiConfigPlanApplyResult {
        valid: bool_at(result, &["valid"]).unwrap_or(true),
        hot_reload: parse_impact_items(result, "hot_reload"),
        requires_restart: parse_impact_items(result, "requires_restart"),
        warnings: string_array_at(result, &["warnings"]),
        errors: string_array_at(result, &["errors"]),
    }
}

fn parse_impact_items(value: &Value, key: &str) -> Vec<GuiConfigImpactItem> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GuiConfigImpactItem {
                        section: string_at(item, &["section", "key", "name"])?,
                        tags: string_array_at(item, &["tags", "affected"]),
                        detail: string_at(item, &["detail", "description", "message"])
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── Utility functions ───────────────────────────────────────────────

pub fn nested_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| value.get(*key))
}

pub fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
    })
}

pub fn string_array_at(value: &Value, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn u64_at(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
    })
}

pub fn bool_at(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|value| {
            value.as_bool().or_else(|| {
                value
                    .as_str()
                    .and_then(|value| match value.to_ascii_lowercase().as_str() {
                        "true" | "yes" | "1" => Some(true),
                        "false" | "no" | "0" => Some(false),
                        _ => None,
                    })
            })
        })
    })
}

/// Strip leading 'v' from version strings so all comparisons are prefix-free.
pub fn normalize_version(version: Option<String>) -> Option<String> {
    version.map(|v| v.strip_prefix('v').unwrap_or(&v).to_string())
}

pub fn normalize_non_empty(value: String, field: &'static str) -> AppResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::invalid_argument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

pub fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

pub fn values_from_container(value: &Value, keys: &[&str]) -> Vec<Value> {
    if let Some(array) = value.as_array() {
        return array.clone();
    }

    for key in keys {
        if let Some(candidate) = value.get(*key) {
            if let Some(array) = candidate.as_array() {
                return array.clone();
            }
            if let Some(object) = candidate.as_object() {
                return object.values().cloned().collect();
            }
        }
    }

    value
        .as_object()
        .map(|object| object.values().cloned().collect())
        .unwrap_or_default()
}

fn endpoint_array_at(value: &Value, key: &str) -> Vec<GuiCapabilityEndpoint> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GuiCapabilityEndpoint {
                        kind: string_at(item, &["kind", "type", "name"])?,
                        enabled: bool_at(item, &["enabled"]).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn protocol_array_at(value: &Value, key: &str) -> Vec<GuiProtocolCapability> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let inbound_tcp_state = protocol_capability_state(
                        item,
                        &["inbound", "tcp"],
                        &["inbound_tcp", "inboundTcp"],
                    );
                    let inbound_udp_state = protocol_capability_state(
                        item,
                        &["inbound", "udp"],
                        &["inbound_udp", "inboundUdp"],
                    );
                    let outbound_tcp_state = protocol_capability_state(
                        item,
                        &["outbound", "tcp"],
                        &["outbound_tcp", "outboundTcp"],
                    );
                    let outbound_udp_state = protocol_capability_state(
                        item,
                        &["outbound", "udp"],
                        &["outbound_udp", "outboundUdp"],
                    );
                    let mux_state = protocol_capability_state(item, &["mux"], &["mux"]);
                    Some(GuiProtocolCapability {
                        name: string_at(item, &["name", "protocol"])?,
                        status: string_at(item, &["status"])
                            .unwrap_or_else(|| "supported".to_string()),
                        inbound_tcp: inbound_tcp_state.supported,
                        inbound_udp: inbound_udp_state.supported,
                        outbound_tcp: outbound_tcp_state.supported,
                        outbound_udp: outbound_udp_state.supported,
                        mux: mux_state.supported,
                        inbound_tcp_state,
                        inbound_udp_state,
                        outbound_tcp_state,
                        outbound_udp_state,
                        mux_state,
                        limitations: string_array_at(item, &["limitations"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn protocol_capability_state(
    item: &Value,
    nested_path: &[&str],
    legacy_keys: &[&str],
) -> GuiCapabilityState {
    if let Some(state) = nested_value(item, nested_path).filter(|value| value.is_object()) {
        let supported = bool_at(state, &["supported"]).unwrap_or(false);
        return GuiCapabilityState {
            supported,
            level: string_at(state, &["level"]).unwrap_or_else(|| {
                if supported {
                    "supported"
                } else {
                    "unsupported"
                }
                .to_string()
            }),
            notes: string_array_at(state, &["notes"]),
        };
    }

    let supported = bool_at(item, legacy_keys).unwrap_or(false);
    GuiCapabilityState {
        supported,
        level: if supported {
            "supported"
        } else {
            "unsupported"
        }
        .to_string(),
        notes: Vec::new(),
    }
}
