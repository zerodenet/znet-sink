use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemovalRuntime {
    pub state: String,
    pub core_instance_id: Option<String>,
    pub config_revision: Option<String>,
    pub managed_pid: Option<u32>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalRelation {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalReplacement {
    pub id: String,
    pub name: String,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalPreview {
    pub target_id: String,
    pub target_name: String,
    pub target_version: String,
    pub active: bool,
    pub source_type: String,
    pub guard: String,
    pub subscriptions: Vec<RemovalRelation>,
    pub replacements: Vec<RemovalReplacement>,
    pub runtime: RemovalRuntime,
    pub blocked_reason: Option<String>,
    pub pending: bool,
    pub override_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RemovalAction {
    Inactive {},
    Replace { replacement_id: String },
    Stop {},
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemovalInput {
    pub target_id: String,
    pub guard: String,
    pub action: RemovalAction,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalResult {
    pub status: String,
    pub message: String,
    pub removed_id: Option<String>,
    pub replacement_id: Option<String>,
    pub runtime_stopped: bool,
}
