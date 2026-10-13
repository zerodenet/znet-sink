//! GUI DTOs: revisions and timestamps retain all u64 bits across the WebView.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketEndpoint {
    pub endpoint_id: String,
    pub generation: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketRoute {
    pub route_id: String,
    pub core_instance_id: String,
    pub config_revision: String,
    pub inbound_tag: String,
    pub outbound_tag: String,
    pub endpoints: Vec<PacketEndpoint>,
    pub source: String,
    pub destination: String,
    pub ip_protocol: u8,
    pub translated: bool,
    pub started_at_unix_ms: String,
    pub state: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketSnapshot {
    pub supported: bool,
    pub can_close: bool,
    pub reason: Option<String>,
    pub close_reason: Option<String>,
    pub profile_id: Option<String>,
    pub core_instance_id: Option<String>,
    pub config_revision: Option<String>,
    pub observed_at_unix_ms: Option<String>,
    pub routes: Vec<PacketRoute>,
    pub total: Option<usize>,
    pub next_offset: Option<usize>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketTarget {
    pub profile_id: Option<String>,
    pub route_id: String,
    pub core_instance_id: String,
    pub config_revision: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketCloseResult {
    /// confirmed / unconfirmed; a transport failure never becomes fake success.
    pub status: String,
    pub message: String,
    pub route: Option<PacketRoute>,
}
