use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Row {
    pub id: String,
    pub enabled: bool,
    pub rule_json: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub kind: &'static str,
    pub tag: Option<String>,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub profile_id: String,
    pub profile_name: String,
    pub binding: Option<String>,
    pub read_only_reason: Option<String>,
    pub stale_reason: Option<String>,
    pub has_local_overlay: bool,
    pub rows: Vec<Row>,
    pub targets: Vec<Target>,
    pub source_updated_at_unix_ms: String,
    pub validation_scope: String,
    pub pending_apply: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Replace,
    RestoreSource,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub profile_id: String,
    pub binding: String,
    pub mode: Mode,
    pub rows: Vec<Row>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_id: String,
    pub profile_id: String,
    pub mode: Mode,
    pub rows: Vec<Row>,
    pub expires_at_unix_ms: String,
    pub state: &'static str,
    pub notice: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub plan_id: String,
    pub profile_id: String,
    pub state: &'static str,
    pub reason: String,
    pub snapshot: Option<Snapshot>,
}
