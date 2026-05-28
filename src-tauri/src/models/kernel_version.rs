use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Beta,
    Nightly,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelRelease {
    pub version: String,
    pub channel: ReleaseChannel,
    pub prerelease: bool,
    pub published_at_unix_ms: Option<u64>,
    pub asset_size_bytes: Option<u64>,
    pub asset_download_url: Option<String>,
    pub release_notes_url: Option<String>,
    pub checksum_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelVersionList {
    pub versions: Vec<KernelRelease>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelDownloadProgress {
    pub version: String,
    pub bytes_downloaded: u64,
    pub bytes_total: Option<u64>,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelInstallResult {
    pub success: bool,
    pub executable_path: String,
    pub version: String,
    pub channel: ReleaseChannel,
    pub checksum_verified: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelVersionDetect {
    pub version: Option<String>,
    pub source: String,
}
