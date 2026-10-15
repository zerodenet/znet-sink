use serde::{Deserialize, Serialize};

/// Paths and native tags only. Never includes source JSON, URLs or artifacts.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub profile_id: Option<String>,
    pub field_path: String,
    pub tag: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Replacement {
    pub from_tag: String,
    pub to_tag: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub reason_code: &'static str,
    pub reason: &'static str,
}
