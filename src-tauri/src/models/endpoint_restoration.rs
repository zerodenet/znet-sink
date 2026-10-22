use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorationTarget {
    pub binding_id: String,
    pub endpoint_id: String,
    pub profile_id: String,
    pub tag: String,
    pub protocol: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub binding_id: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub plan_id: String,
    pub mode: RestoreMode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    SavePending,
    ApplyRuntime,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorationPlan {
    pub plan_id: String,
    pub target: RestorationTarget,
    pub source_digest: String,
    pub source_updated_at_unix_ms: String,
    pub previous_effective_digest: String,
    pub effective_digest: String,
    pub removed_fields: Vec<String>,
    pub runtime_available: bool,
    pub impact: String,
    pub client_scope: crate::client_core::ClientScope,
    pub created_at_unix_ms: String,
    pub expires_at_unix_ms: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorePhase {
    Prepared,
    SavedPending,
    Applying,
    Applied,
    Clearing,
    RuntimeConfirmed,
    Confirmed,
    ResultUnknown,
    Rejected,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorationReceipt {
    pub operation_id: String,
    pub target: RestorationTarget,
    pub phase: RestorePhase,
    pub local_saved: bool,
    pub runtime_confirmed: bool,
    pub apply_identity: Option<RuntimeReceipt>,
    pub endpoint: Option<Value>,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReceipt {
    pub core_instance_id: String,
    pub config_revision: String,
}
