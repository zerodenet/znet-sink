use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileReleaseChannel {
    #[default]
    Stable,
    Preview,
    Test,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MobileReleaseSource {
    Client,
    Kernel,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleasePreferences {
    #[serde(default)]
    pub client_auto_check: bool,
    #[serde(default)]
    pub client_channel: MobileReleaseChannel,
    #[serde(default)]
    pub kernel_channel: MobileReleaseChannel,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePreferencesSnapshot {
    pub preferences: ReleasePreferences,
    pub expected_revision: String,
    pub scheduler_state: &'static str,
    pub operation: Option<ReleasePreferenceOperation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleasePreferenceReceipt {
    pub owner_operation: String,
    pub before_epoch: String,
    pub after_epoch: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleasePreferenceOperation {
    pub operation_id: String,
    pub expected_revision: String,
    pub preferences: ReleasePreferences,
    pub state: String,
    pub acknowledged: bool,
    pub reason: Option<String>,
    pub receipt: Option<ReleasePreferenceReceipt>,
    #[serde(default)]
    pub review_binding: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileRelease {
    pub tag_name: String,
    pub version: String,
    pub channel: MobileReleaseChannel,
    /// Preserve the existing core stable/beta/nightly contract without renaming it.
    pub kernel_channel: Option<crate::models::kernel_version::ReleaseChannel>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub notes_truncated: bool,
    pub release_url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileReleaseCatalog {
    pub source: MobileReleaseSource,
    pub channel: MobileReleaseChannel,
    pub platform: &'static str,
    pub checked_at_unix_ms: String,
    pub releases: Vec<MobileRelease>,
    pub catalog_limit: usize,
    pub target_status: &'static str,
    pub target_reason: &'static str,
    pub request_id: String,
    pub response_status: u16,
    pub reused_validated_cache: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseScheduleSnapshot {
    pub host_session_id: String,
    pub revision: String,
    pub foreground: bool,
    pub checking: bool,
    pub enabled: bool,
    pub client_channel: MobileReleaseChannel,
    pub last_attempted_at_unix_ms: Option<String>,
    pub last_success_at_unix_ms: Option<String>,
    pub next_due_at_unix_ms: Option<String>,
    pub retry_at_unix_ms: Option<String>,
    pub outcome: String,
    pub reason: Option<String>,
    pub storage_error: bool,
    pub foreground_scheduling: &'static str,
    pub background_scheduling: &'static str,
    pub cached_catalog: Option<MobileReleaseCatalog>,
    pub cache_current: bool,
    pub preference_operation: Option<ReleasePreferenceOperation>,
}
