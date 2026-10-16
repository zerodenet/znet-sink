use crate::models::gui_core::GuiConfigPlanApplyResult;
use serde::{Deserialize, Serialize, Serializer};

pub(crate) fn decimal<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigApplyStrategy {
    HotReload,
    Restart,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigWorkspaceDraftInput {
    pub binding: String,
    pub source_json: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigWorkspaceApplyInput {
    pub plan_id: String,
    pub strategy: ConfigApplyStrategy,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRuntimeIdentityView {
    pub core_instance_id: String,
    #[serde(serialize_with = "decimal")]
    pub config_revision: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTransactionReceipt {
    #[serde(serialize_with = "decimal")]
    pub transaction_id: u64,
    pub profile_id: String,
    pub strategy: ConfigApplyStrategy,
    pub state: &'static str,
    pub effective_digest: String,
    pub started_at_unix_ms: u64,
    pub completed_at_unix_ms: u64,
    pub runtime_identity: ConfigRuntimeIdentityView,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspaceRuntimeView {
    pub running: Option<bool>,
    pub identity: Option<ConfigRuntimeIdentityView>,
    pub effective_digest: String,
    pub confirmed: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspaceSnapshot {
    pub profile_id: String,
    pub profile_name: String,
    pub binding: Option<String>,
    pub read_only_reason: Option<String>,
    pub source_updated_at_unix_ms: String,
    pub source_json: Option<String>,
    pub local_edits_json: Option<String>,
    pub effective_json: Option<String>,
    pub pending_apply: bool,
    pub pending_operation: Option<ConfigWorkspaceOperation>,
    pub composition: Option<crate::configuration::CompositionReport>,
    pub runtime: ConfigWorkspaceRuntimeView,
    pub last_transaction: Option<ConfigTransactionReceipt>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspacePlan {
    #[serde(flatten)]
    pub impact: GuiConfigPlanApplyResult,
    pub plan_id: String,
    pub binding: String,
    pub source_json: String,
    pub effective_json: String,
    pub effective_digest: String,
    pub expires_at_unix_ms: String,
    pub runtime_validated: bool,
    pub apply_unavailable_reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspaceSaveResult {
    pub state: &'static str,
    pub snapshot: ConfigWorkspaceSnapshot,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspaceOperation {
    pub plan_id: String,
    pub state: &'static str,
    pub receipt: Option<ConfigTransactionReceipt>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspaceRecoveryReview {
    pub plan_id: String,
    pub review_id: Option<String>,
    pub state: &'static str,
    pub can_complete_local: bool,
    pub reason: String,
    pub receipt: Option<ConfigTransactionReceipt>,
    pub expires_at_unix_ms: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigWorkspaceCompleteInput {
    pub plan_id: String,
    pub review_id: String,
}
