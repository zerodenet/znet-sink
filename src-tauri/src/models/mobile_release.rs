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
}
