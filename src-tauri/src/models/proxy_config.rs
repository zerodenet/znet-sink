use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::subscription::ManagedSubscriptionSource;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfigProfile {
    pub id: String,
    pub name: String,
    pub kernel: String,
    pub format: String,
    pub path: Option<String>,
    pub content: Option<Value>,
    pub active: bool,
    /// Stable plugin ownership for a configuration projected from a managed
    /// subscription. Manual configurations never populate this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_source: Option<ManagedSubscriptionSource>,
    pub updated_at_unix_ms: u64,
    pub capabilities: ProxyConfigCapabilities,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfigUpsert {
    pub id: Option<String>,
    pub name: String,
    pub kernel: Option<String>,
    pub format: Option<String>,
    pub path: Option<String>,
    pub content: Option<Value>,
    pub active: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfigImport {
    pub id: Option<String>,
    pub name: String,
    pub kernel: Option<String>,
    pub format: Option<String>,
    pub path: Option<String>,
    pub content: Option<String>,
    pub active: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfigCapabilities {
    pub has_proxy_nodes: bool,
    pub has_proxy_groups: bool,
    pub has_route_rules: bool,
    pub has_rule_sets: bool,
    pub has_selector: bool,
    pub has_url_test: bool,
    pub feature_keys: Vec<String>,
}
