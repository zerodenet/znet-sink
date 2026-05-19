use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionProfile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub kernel: String,
    pub format: String,
    pub target_proxy_config_id: Option<String>,
    pub updated_at_unix_ms: u64,
    pub last_sync_at_unix_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionUpsert {
    pub id: Option<String>,
    pub name: String,
    pub url: String,
    pub enabled: Option<bool>,
    pub kernel: Option<String>,
    pub format: Option<String>,
    pub target_proxy_config_id: Option<String>,
}
