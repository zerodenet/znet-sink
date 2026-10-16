use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub path: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub name: String,
    pub references: Vec<Reference>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub profile_id: String,
    pub profile_name: String,
    pub binding: Option<String>,
    pub read_only_reason: Option<String>,
    pub servers: Vec<Server>,
    pub notice: String,
    pub pending_apply: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub profile_id: String,
    pub binding: String,
    pub server_name: String,
    pub replacement_name: Option<String>,
    /// None retains the existing delete-and-migrate intent. A rename cannot
    /// also select an existing replacement backend.
    pub rename_to: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_id: String,
    pub profile_id: String,
    pub server_name: String,
    pub replacement_name: Option<String>,
    pub rename_to: Option<String>,
    pub references: Vec<Reference>,
    pub notice: String,
    pub state: &'static str,
    pub expires_at_unix_ms: String,
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
