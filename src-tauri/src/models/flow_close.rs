use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub flow_id: String,
    pub target_key: String,
    pub started_at_unix_ms: String,
    pub network: String,
    pub destination: String,
    pub can_close: bool,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub inventory_id: String,
    pub core_instance_id: String,
    pub config_revision: String,
    pub client_scope_generation: String,
    pub observed_at_unix_ms: String,
    pub targets: Vec<Target>,
    pub limit: usize,
    pub truncated: bool,
    pub notice: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedTarget {
    pub flow_id: String,
    pub target_key: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub inventory_id: String,
    pub targets: Vec<SelectedTarget>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTarget {
    #[serde(flatten)]
    pub target: Target,
    pub state: &'static str,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub plan_id: String,
    pub core_instance_id: String,
    pub config_revision: String,
    pub expires_at_unix_ms: String,
    pub targets: Vec<PreviewTarget>,
    pub notice: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Closed,
    Ended,
    Rejected,
    Unknown,
    Stale,
    NotSubmitted,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub flow_id: String,
    pub outcome: Outcome,
    pub reason: String,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub closed: usize,
    pub ended: usize,
    pub rejected: usize,
    pub unknown: usize,
    pub stale: usize,
    pub not_submitted: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultView {
    pub plan_id: String,
    pub state: &'static str,
    pub items: Vec<Item>,
    pub counts: Counts,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub plan_id: String,
    pub state: &'static str,
    pub result: Option<ResultView>,
}
