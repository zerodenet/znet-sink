use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::core_config::CoreConfigSnapshot;
use crate::models::core_process::CoreProcessStatus;
use crate::services::system_proxy::SystemProxyStatus;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiCoreOverview {
    pub process: CoreProcessStatus,
    pub available: bool,
    pub health: Option<GuiCoreHealth>,
    /// Kernel config snapshot (listeners, outbounds, rules count, etc.)
    pub config: Option<serde_json::Value>,
    pub stats: GuiTrafficStats,
    pub policy_groups: Vec<GuiPolicyGroup>,
    pub capabilities: GuiZeroCapabilities,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConnectionStatus {
    pub connected: bool,
    pub stage: String,
    pub core_available: bool,
    pub process: CoreProcessStatus,
    pub system_proxy: Option<SystemProxyStatus>,
    pub health: Option<GuiCoreHealth>,
    pub stats: GuiTrafficStats,
    pub active_proxy_config_id: Option<String>,
    pub local_proxy_host: String,
    pub local_proxy_port: u16,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GuiProxyMode {
    Global,
    Rule,
    Direct,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSetProxyModeInput {
    pub mode: GuiProxyMode,
    pub global_outbound: Option<String>,
    pub restart_core: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiProxyModeStatus {
    pub mode: Option<GuiProxyMode>,
    pub active_proxy_config_id: Option<String>,
    pub global_outbound: Option<String>,
    pub rule_count: usize,
    pub has_route: bool,
    pub exported: bool,
    pub core_running: bool,
    pub restarted_core: bool,
    pub system_proxy_enabled: bool,
    pub requires_reconnect: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSelfTestSnapshot {
    pub ready: bool,
    /// Human-readable messages for blocking (Fail) checks.
    pub blocking_issues: Vec<String>,
    pub warning_count: usize,
    pub active_proxy_config_id: Option<String>,
    pub active_proxy_config_name: Option<String>,
    pub core_config: CoreConfigSnapshot,
    pub proxy_mode: GuiProxyModeStatus,
    pub connection: GuiConnectionStatus,
    pub checks: Vec<GuiSelfTestCheck>,
    pub suggested_flow: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSelfTestCheck {
    pub key: String,
    pub status: GuiSelfTestCheckStatus,
    pub message: String,
    pub details: Option<Value>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GuiSelfTestCheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiCoreHealth {
    pub healthy: bool,
    pub engine_version: Option<String>,
    pub started_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTrafficStats {
    pub active_sessions: u64,
    pub total_started: u64,
    pub completed_sessions: u64,
    pub failed_sessions: u64,
    pub blocked_sessions: u64,
    pub direct_sessions: u64,
    pub chained_sessions: u64,
    pub bytes_up: u64,
    pub bytes_down: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTrafficRates {
    pub upload_bps: u64,
    pub download_bps: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTrafficSnapshot {
    pub totals: GuiTrafficStats,
    pub rates: GuiTrafficRates,
    pub sampled_at_unix_ms: u64,
    pub interval_ms: Option<u64>,
    pub source: String,
    pub stable: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiZeroCapabilities {
    pub available: bool,
    pub api_version: Option<String>,
    pub schema_version: Option<String>,
    pub features: Vec<String>,
    pub permissions: Vec<String>,
    pub adapters: Vec<GuiCapabilityEndpoint>,
    pub sinks: Vec<GuiCapabilityEndpoint>,
    /// Protocol capability matrix — inbound/outbound TCP/UDP support,
    /// MUX status, and limitation codes.
    pub protocols: Vec<GuiProtocolCapability>,
    /// Build-time compiled features (e.g. "tun", "shadowsocks").
    pub build_features: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiCapabilityEndpoint {
    pub kind: String,
    pub enabled: bool,
}

/// Protocol capability entry from the kernel's capabilities response.
/// Reports whether a specific protocol is supported, partial, or experimental,
/// along with TCP/UDP inbound/outbound support and any limitations.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiProtocolCapability {
    /// Protocol name (e.g. "shadowsocks", "vmess", "trojan").
    pub name: String,
    /// "supported" | "partial" | "experimental"
    pub status: String,
    pub inbound_tcp: bool,
    pub inbound_udp: bool,
    pub outbound_tcp: bool,
    pub outbound_udp: bool,
    pub mux: bool,
    /// Opaque limitation codes from the kernel (e.g. "no_udp_relay").
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicyGroup {
    pub name: String,
    pub kind: String,
    pub selected: Option<String>,
    pub outbounds: Vec<GuiPolicyMember>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicyMember {
    pub tag: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub selected: bool,
    pub alive: Option<bool>,
    pub delay_ms: Option<u64>,
}

/// Proxy node extracted from the active proxy config file (static data).
/// Available even when the core isn't running — used for the node list
/// and selector dropdown.  Runtime status (selected, latency, alive) is
/// layered on top from the core's Policies query when connected.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProxyNode {
    pub tag: String,
    pub protocol: String,
    pub is_selector: bool,
    /// Server hostname / IP (from `protocol.server`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// Remote port (from `protocol.port`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Whether the outbound supports UDP relay (from `protocol.udp`).
    /// Defaults to `Some(true)` for protocols that are UDP-capable by design
    /// (e.g. `hysteria*`, `tuic`, `wireguard`) when the field is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp: Option<bool>,
    /// Transport network type: `tcp`, `ws`, `grpc`, `h2`, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// Whether TLS is enabled on the outbound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<bool>,
    /// Server Name Indication (from `protocol.sni`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    /// Cipher / encryption algorithm (from `protocol.cipher` / `security`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cipher: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConnectionListOptions {
    pub limit: Option<u32>,
    pub inbound_tag: Option<String>,
    pub principal_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConnectionList {
    pub items: Vec<GuiConnection>,
    pub total: Option<u64>,
    pub limit: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConnection {
    pub flow_id: String,
    pub network: String,
    pub source: Option<String>,
    pub destination: String,
    pub inbound_tag: Option<String>,
    pub outbound_tag: Option<String>,
    pub policy_tag: Option<String>,
    pub route_mode: Option<String>,
    pub outcome: Option<String>,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub throughput_up_bps: Option<u64>,
    pub throughput_down_bps: Option<u64>,
    pub started_at_unix_ms: Option<u64>,
    pub updated_at_unix_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicySelectionResult {
    pub policy_tag: String,
    pub target_tag: String,
    pub selected: Option<String>,
    pub accepted: bool,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTargetProbeResult {
    pub target_tag: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub server: Option<String>,
    pub port: Option<u64>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConnectionCloseResult {
    pub flow_id: String,
    pub closed: bool,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiFeatureStatus {
    pub key: String,
    pub supported: bool,
    pub enabled: bool,
    pub state: String,
    pub reason: Option<String>,
}

/// A single impact item from `config.plan_apply` — one section of config
/// that will be affected by the proposed change.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConfigImpactItem {
    /// Top-level config section (e.g. "outbounds", "listeners", "rules", "tun").
    pub section: String,
    /// Specific tags/identifiers within the section that changed.
    pub tags: Vec<String>,
    /// Human-readable description of the change.
    pub detail: String,
}

/// Result of `config.plan_apply` — a dry-run impact analysis before
/// actually applying a config change to the running kernel.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConfigPlanApplyResult {
    /// Whether the proposed config is syntactically and semantically valid.
    pub valid: bool,
    /// Sections that can be hot-reloaded without restarting the kernel.
    pub hot_reload: Vec<GuiConfigImpactItem>,
    /// Sections that require a kernel restart to take effect.
    pub requires_restart: Vec<GuiConfigImpactItem>,
    /// Non-blocking warnings about side effects.
    pub warnings: Vec<String>,
    /// Validation errors (present when `valid` is false).
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiEventSubscription {
    pub generation: u64,
    pub event_name: &'static str,
    pub status_event_name: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiEventPayload {
    pub generation: u64,
    pub event: GuiEvent,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiEventStatus {
    pub generation: u64,
    pub status: &'static str,
    pub error: Option<crate::errors::AppError>,
    pub response: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiEvent {
    pub event_type: String,
    pub source_event_type: String,
    pub event_id: Option<String>,
    pub sequence: Option<u64>,
    pub occurred_at_unix_ms: Option<u64>,
    pub payload: GuiEventData,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "camelCase")]
pub enum GuiEventData {
    CoreStatus(GuiCoreHealth),
    CoreWarning(GuiWarningEvent),
    ConfigChanged(GuiConfigChangedEvent),
    Connection(GuiConnection),
    PolicySelected(GuiPolicySelectedEvent),
    PolicyProbeCompleted(GuiPolicyProbeCompletedEvent),
    TrafficStats(GuiTrafficStats),
    /// TUN virtual network interface lifecycle (v0.0.5+)
    TunStatus(GuiTunStatusEvent),
    /// Network stack status change — SystemStack / proxy stack (v0.0.5+)
    StackStatus(GuiStackStatusEvent),
    /// IPC client connection lifecycle (v0.0.11+)
    IpcStatus(GuiIpcStatusEvent),
    Unknown(GuiUnknownEvent),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiWarningEvent {
    pub code: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicySelectedEvent {
    pub policy_tag: String,
    pub policy_kind: Option<String>,
    pub selected: String,
    pub previous: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConfigChangedEvent {
    pub changed_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicyProbeCompletedEvent {
    pub policy_tag: String,
    pub selected: Option<String>,
    pub members: Vec<GuiPolicyMember>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiTunStatusEvent {
    /// "started" | "stopped" | "error"
    pub state: String,
    pub interface_name: Option<String>,
    pub address: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiStackStatusEvent {
    /// "started" | "stopped" | "degraded"
    pub state: String,
    /// "system" (SystemStack) | "proxy" | "mixed"
    pub mode: Option<String>,
    pub message: Option<String>,
}

/// IPC client connection lifecycle event (ipc.connected / ipc.disconnected).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiIpcStatusEvent {
    /// Number of active IPC connections after this event.
    pub active: u32,
    /// Named pipe path (Windows) or peer address (Unix).
    pub pipe: Option<String>,
    /// Error details on abnormal disconnection.
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiUnknownEvent {
    pub message: Option<String>,
    pub summary: Option<Value>,
}
