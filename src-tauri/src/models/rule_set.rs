use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub built_in: bool,
    #[serde(default)]
    pub provenance: Option<RuleSetProvenance>,
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

/// Lightweight list projection. Large source-managed rule arrays never cross
/// the Tauri IPC boundary; callers fetch a full profile only for small local
/// rule sets that can be edited safely.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub built_in: bool,
    pub provenance: Option<RuleSetProvenance>,
    pub common_binding: Option<CommonRuleBinding>,
    pub editable_rule_count: usize,
    pub source: Option<RuleSetSource>,
    pub source_state: RuleSetSourceState,
    pub artifact: Option<ZrsArtifact>,
    pub updated_at_unix_ms: u64,
    pub last_sync_at_unix_ms: Option<u64>,
    pub last_error: Option<String>,
}

impl From<&RuleSetProfile> for RuleSetSummary {
    fn from(profile: &RuleSetProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            enabled: profile.enabled,
            built_in: profile.built_in,
            provenance: profile.provenance.clone(),
            common_binding: profile.common_binding.clone(),
            editable_rule_count: profile
                .semantic_ir
                .get("rules")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
            source: profile.source.clone(),
            source_state: profile.source_state.clone(),
            artifact: profile.artifact.clone(),
            updated_at_unix_ms: profile.updated_at_unix_ms,
            last_sync_at_unix_ms: profile.last_sync_at_unix_ms,
            last_error: profile.last_error.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommonRuleAction {
    Final,
    Proxy,
    Direct,
    Reject,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetProvenance {
    pub repository: String,
    pub revision: String,
    pub license: String,
    pub source_url: String,
    pub source_sha256: String,
    pub ir_sha256: String,
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

/// A rule-set reference exactly as it appears in the composed Zero config.
/// DNS dispatch consumes the effective tag, not the GUI resource id.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRuleSetOption {
    pub tag: String,
    pub name: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetSyncAllOutcome {
    pub total: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub failed: usize,
}
