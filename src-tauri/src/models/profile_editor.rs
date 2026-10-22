use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub target_id: Option<String>,
    pub create_target_id: Option<String>,
    pub binding: Option<String>,
    pub name: String,
    pub source_json: Option<String>,
    pub read_only_reason: Option<String>,
    pub source_updated_at_unix_ms: Option<String>,
    pub validation_scope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DraftInput {
    pub id: Option<String>,
    pub binding: String,
    pub source_json: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveInput {
    pub id: Option<String>,
    pub binding: String,
    pub name: String,
    pub source_json: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Validated {
    pub state: &'static str,
    pub source_json: String,
    pub validation_scope: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub state: &'static str,
    pub snapshot: Snapshot,
}
