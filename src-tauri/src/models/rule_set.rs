use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub managed_by_subscription_id: Option<String>,
    #[serde(default)]
    pub common_binding: Option<CommonRuleBinding>,
    /// Canonical Zero Rule IR v1. This is the only user-visible/editable rule model.
    pub semantic_ir: Value,
    #[serde(default)]
    pub source: Option<RuleSetSource>,
    #[serde(default)]
    pub source_state: RuleSetSourceState,
    #[serde(default)]
    pub artifact: Option<ZrsArtifact>,
    pub updated_at_unix_ms: u64,
    #[serde(default)]
    pub last_sync_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommonRuleAction {
    Final,
    Direct,
    Reject,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommonRuleBinding {
    pub enabled: bool,
    pub action: CommonRuleAction,
    pub order: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonRuleBindingInput {
    pub rule_set_id: String,
    pub enabled: bool,
    pub action: CommonRuleAction,
    pub order: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonRuleInjectionStatus {
    pub enabled: bool,
    pub effective: bool,
    pub mode: Option<String>,
    pub eligible_count: usize,
    pub injected_count: usize,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetSource {
    pub url: String,
    /// Adapter hint only. It never becomes part of the semantic asset or kernel payload.
    pub format: String,
    #[serde(default)]
    pub update_interval_secs: Option<u64>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetSourceState {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_sha256: Option<String>,
    pub content_bytes: Option<u64>,
    pub last_checked_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZrsArtifact {
    pub path: String,
    pub major_version: u16,
    pub minor_version: u16,
    pub checksum: u32,
    pub file_size: u64,
    pub entry_count: u64,
    pub built_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetUpsert {
    pub id: Option<String>,
    pub name: String,
    pub enabled: Option<bool>,
    pub semantic_ir: Option<Value>,
    pub source: Option<RuleSetSource>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetKernelPayload {
    pub id: String,
    pub name: String,
    pub zrs_path: String,
    pub checksum: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetSyncAllOutcome {
    pub total: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub failed: usize,
}
