//! Host-owned fixed uninstall review; no plugin source, credentials or real paths.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    pub resource_id: String,
    pub kind: String,
    pub name: String,
    pub ownership: String,
    pub dependencies: Vec<String>,
    pub protected_reason: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub plan_id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub version: String,
    pub package_digest: String,
    pub publisher_id: String,
    pub publisher_fingerprint: String,
    pub host_session_id: String,
    pub binding: String,
    pub expires_at_unix_ms: String,
    pub can_execute: bool,
    pub blocked_reasons: Vec<String>,
    pub active_profile: Option<Profile>,
    pub replacement_candidates: Vec<Profile>,
    pub requires_manual_switch: bool,
    pub requires_stop: bool,
    pub resources: Vec<Resource>,
    pub notice: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    pub resource_id: String,
    pub kind: String,
    pub name: String,
    pub state: String,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Status {
    pub operation_id: String,
    pub plan_id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub version: String,
    pub package_digest: String,
    pub publisher_fingerprint: String,
    pub state: String,
    pub reason: Option<String>,
    pub started_at_unix_ms: Option<String>,
    pub finished_at_unix_ms: Option<String>,
    pub items: Vec<Item>,
    pub can_retry: bool,
    pub notice: String,
}
