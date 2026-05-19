use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetProfile {
    pub id: String,
    pub name: String,
    pub format: String,
    pub enabled: bool,
    pub source: RuleSetSource,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetSource {
    pub kind: String,
    pub url: Option<String>,
    pub path: Option<String>,
    pub content: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSetUpsert {
    pub id: Option<String>,
    pub name: String,
    pub format: Option<String>,
    pub enabled: Option<bool>,
    pub source: RuleSetSource,
}
