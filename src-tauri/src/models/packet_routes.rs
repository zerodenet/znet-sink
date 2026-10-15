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
    pub close_operation_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketSnapshot {
    pub offset: u32,
    pub limit: u16,
    pub client_scope_generation: u64,
    pub host_session_id: String,
    pub blocked_targets: Vec<PacketPhysicalTarget>,
    pub close_ledger_full: bool,
    pub close_records: Vec<PacketCloseRecord>,
    pub close_ledger_recovery: PacketLedgerRecovery,
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
    pub client_scope_generation: u64,
    pub host_session_id: String,
    #[serde(default)]
    pub close_operation_id: Option<String>,
    pub route_id: String,
    pub core_instance_id: String,
    pub config_revision: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketCloseResult {
    /// confirmed / unconfirmed; a transport failure never becomes fake success.
    pub status: String,
    pub operation_id: Option<String>,
    pub message: String,
    pub route: Option<PacketRoute>,
}

/// Host view lifetime is independent of a physical Packet conversation.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketPageIdentity {
    pub profile_id: Option<String>,
    pub core_instance_id: String,
    pub config_revision: String,
    pub client_scope_generation: u64,
    pub host_session_id: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketPageQuery {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "packet_page_limit")]
    pub limit: u16,
    pub expected: Option<PacketPageIdentity>,
}
fn packet_page_limit() -> u16 {
    500
}
impl Default for PacketPageQuery {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: packet_page_limit(),
            expected: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PacketPhysicalTarget {
    pub core_instance_id: String,
    pub route_id: String,
    pub config_revision: String,
}
impl From<&PacketTarget> for PacketPhysicalTarget {
    fn from(target: &PacketTarget) -> Self {
        Self {
            core_instance_id: target.core_instance_id.clone(),
            route_id: target.route_id.clone(),
            config_revision: target.config_revision.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketCloseRecord {
    pub target: PacketPhysicalTarget,
    pub operation_id: String,
    pub origin_process_id: String,
    pub origin_profile_id: Option<String>,
    pub origin_client_scope_generation: String,
    pub reserved_at_unix_ms: String,
    pub recovered: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketLedgerRecovery {
    pub state: &'static str,
    pub reason: Option<String>,
    pub recovered_count: Option<usize>,
}

/// Closed metadata projection. Never includes target IDs, record bodies,
/// parser/OS messages or paths; inspection cannot authorize or clear a close.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketLedgerInspection {
    pub host_session_id: Option<String>,
    pub sampled_at_unix_ms: String,
    pub state: &'static str,
    pub reason_code: &'static str,
    pub summary: &'static str,
    pub read_only: bool,
    pub barriers_retained: bool,
    pub new_close_blocked: bool,
    pub in_memory_barrier_count: Option<usize>,
    pub publication_unconfirmed: bool,
    pub evidence: PacketLedgerEvidence,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketLedgerEvidence {
    pub journal_present: Option<bool>,
    pub marker_present: Option<bool>,
    pub marker_state: &'static str,
    pub lock_state: &'static str,
    pub journal_bytes: Option<String>,
    pub journal_sha256: Option<String>,
    pub schema_version: Option<u32>,
    pub verified_record_count: Option<usize>,
    pub snapshot_stable: bool,
}

impl Default for PacketLedgerEvidence {
    fn default() -> Self {
        Self {
            journal_present: None,
            marker_present: None,
            marker_state: "not-read",
            lock_state: "not-checked",
            journal_bytes: None,
            journal_sha256: None,
            schema_version: None,
            verified_record_count: None,
            snapshot_stable: false,
        }
    }
}
