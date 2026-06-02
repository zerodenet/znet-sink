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
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiCapabilityEndpoint {
    pub kind: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicyGroup {
    pub tag: String,
    pub kind: String,
    pub selected: Option<String>,
    pub members: Vec<GuiPolicyMember>,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiPolicyMember {
    pub tag: String,
    pub kind: Option<String>,
    pub selected: bool,
    pub alive: Option<bool>,
    pub delay_ms: Option<u64>,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiUnknownEvent {
    pub message: Option<String>,
    pub summary: Option<Value>,
}
