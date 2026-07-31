use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// User-selected targets for selector policy groups. This runtime
    /// preference belongs to the subscription and survives kernel restarts.
    #[serde(default)]
    pub policy_selections: BTreeMap<String, String>,
    /// Auto-sync interval in seconds. When `Some` and `enabled`, the
    /// background scheduler will re-sync this subscription once the
    /// interval elapses since `last_sync_at_unix_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_interval_secs: Option<u64>,
    /// Custom User-Agent header sent when fetching this subscription.
    /// When `None`, a sensible default is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Number of usable proxy nodes detected during the last sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_count: Option<u32>,
    /// Bytes uploaded through the subscription, parsed from the
    /// `subscription-userinfo` response header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// Subscription expiry, as a Unix timestamp in milliseconds
    /// (converted from the header's seconds value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_at_unix_ms: Option<u64>,
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
    pub update_interval_secs: Option<u64>,
    pub user_agent: Option<String>,
}

/// Fields that a sync operation may refresh on the stored profile.
/// Kept as a separate struct so the command/service layers stay
/// decoupled from the full `SubscriptionProfile` shape.
#[derive(Clone, Debug, Default)]
pub struct SyncMetadata {
    pub node_count: Option<u32>,
    pub upload_bytes: Option<u64>,
    pub download_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub expire_at_unix_ms: Option<u64>,
}
