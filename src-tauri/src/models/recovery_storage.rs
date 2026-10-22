//! Read-only local evidence; never a runtime receipt or a recovery permit.
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStorageEvidence {
    pub database_present: Option<bool>,
    pub database_bytes: Option<String>,
    pub application_id: Option<i32>,
    pub schema_version: Option<i32>,
    pub metadata_rows: Option<usize>,
    pub pending_rows: Option<usize>,
    pub candidate_bodies_read: bool,
    pub snapshot_stable: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStorageInspection {
    pub host_session_id: Option<String>,
    pub sampled_at_unix_ms: String,
    pub read_only: bool,
    pub barriers_retained: bool,
    pub state: &'static str,
    pub reason_code: &'static str,
    pub summary: &'static str,
    pub evidence: WorkspaceStorageEvidence,
}
