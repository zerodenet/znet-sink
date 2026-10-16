use super::rule_references::{Blocker, Reference};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub unavailable_reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub profile_id: String,
    pub profile_name: String,
    pub rule_set_id: String,
    pub rule_set_name: Option<String>,
    pub resource_exists: bool,
    pub tag: String,
    pub binding: Option<String>,
    pub references: Vec<Reference>,
    pub blockers: Vec<Blocker>,
    pub targets: Vec<Target>,
    pub common_binding_removed: bool,
    pub notice: String,
    pub pending_apply: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub profile_id: String,
    pub rule_set_id: String,
    pub binding: String,
    pub target_id: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub plan_id: String,
    pub profile_id: String,
    pub rule_set_id: String,
    pub rule_set_name: String,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub from_tag: String,
    pub to_tag: Option<String>,
    pub references: Vec<Reference>,
    pub common_binding_removed: bool,
    pub expires_at_unix_ms: String,
    pub state: &'static str,
    pub notice: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub plan_id: String,
    pub profile_id: String,
    pub rule_set_id: String,
    pub state: &'static str,
    pub reason: String,
    pub app_config_published: bool,
    pub rule_set_removed: bool,
    pub after_binding: Option<String>,
}
