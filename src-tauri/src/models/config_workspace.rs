use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::gui_core::GuiConfigPlanApplyResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigApplyStrategy {
    HotReload,
    Restart,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspaceApplyInput {
    pub profile_id: String,
    pub source_updated_at_unix_ms: u64,
    pub source_config: Value,
    pub strategy: ConfigApplyStrategy,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRuntimeIdentityView {
    pub core_instance_id: String,
    pub config_revision: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTransactionReceipt {
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
    pub running: bool,
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
    pub source_updated_at_unix_ms: u64,
    pub source_config: Value,
    pub local_edits: Value,
    pub effective_config: Value,
    pub composition: Option<crate::configuration::CompositionReport>,
    pub runtime: ConfigWorkspaceRuntimeView,
    pub last_transaction: Option<ConfigTransactionReceipt>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigWorkspacePlan {
    #[serde(flatten)]
    pub impact: GuiConfigPlanApplyResult,
    pub effective_config: Value,
    pub effective_digest: String,
}
