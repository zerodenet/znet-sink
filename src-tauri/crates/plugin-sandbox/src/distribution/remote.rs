use super::{
    directory::{Directory, Registration},
    package::verify,
    Result, MAX_PACKAGE_BYTES,
};
use crate::contract::sha256;
use reqwest::{blocking::Client, Url};
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};
pub const DIRECTORY_URL: &str =
    "https://raw.githubusercontent.com/zerodenet/plugins/main/catalogs/znet-sink.json";
type Fetch<'a> = dyn Fn(&str, usize) -> Result<Vec<u8>> + 'a;
pub struct Remote<'a> {
    fetch: Option<Box<Fetch<'a>>>,
    client: Client,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub published_at: Option<String>,
    pub html_url: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseMetadata {
    pub schema_version: u32,
    pub host: String,
    pub plugin_id: String,
    pub version: String,
    pub asset: String,
    pub sha256: String,
}
impl<'a> Remote<'a> {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("ZNet-Sink-Plugin/1")
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || !allowed(attempt.url()) {
                    attempt.error("untrusted release redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self {
            client,
            fetch: None,
        })
    }
    /// Route downloads through the host's permission-aware HTTP executor.
    pub fn with_fetch(fetch: impl Fn(&str, usize) -> Result<Vec<u8>> + 'a) -> Result<Self> {
        let mut remote = Self::new()?;
        remote.fetch = Some(Box::new(fetch));
        Ok(remote)
    }
    pub fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        let url = Url::parse(url)?;
        if !allowed(&url) {
            return Err("untrusted release URL".into());
        }
        if let Some(fetch) = &self.fetch {
            let bytes = fetch(url.as_str(), limit)?;
            if bytes.len() > limit {
                return Err("download exceeds limit".into());
            }
            return Ok(bytes);
        }
        let response = self.client.get(url).send()?.error_for_status()?;
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err("download exceeds limit".into());
        }
        let mut bytes = Vec::new();
        response.take((limit + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err("download exceeds limit".into());
        }
        Ok(bytes)
    }
    pub fn directory(&self) -> Result<Directory> {
        Directory::parse(&self.get(DIRECTORY_URL, 1024 * 1024)?)
    }
    /// Explicit bounded discovery. Older releases can be selected directly by tag.
    pub fn releases(&self, registration: &Registration) -> Result<Vec<Release>> {
        let path = registration.repository_path()?;
        let releases: Vec<Release> = serde_json::from_slice(&self.get(
            &format!("https://api.github.com/repos/{path}/releases?per_page=100"),
            2 * 1024 * 1024,
        )?)?;
        Ok(releases
            .into_iter()
            .filter(|r| !r.draft && r.published_at.is_some())
            .collect())
    }
    pub fn release(&self, registration: &Registration, tag: &str) -> Result<Release> {
        if tag.is_empty() || tag.len() > 128 {
            return Err("invalid release tag".into());
        }
        let mut url = Url::parse(&format!(
            "https://api.github.com/repos/{}/releases/tags/",
            registration.repository_path()?
        ))?;
        url.path_segments_mut()
            .map_err(|_| "invalid release base")?
            .pop_if_empty()
            .push(tag);
        let release: Release = serde_json::from_slice(&self.get(url.as_str(), 512 * 1024)?)?;
        if release.draft || release.published_at.is_none() || release.tag_name != tag {
            return Err("release is not published".into());
        }
        Ok(release)
    }
    pub fn download(&self, registration: &Registration, release: &Release) -> Result<Vec<u8>> {
        let metadata_asset = asset(
            registration,
            release,
            &registration.release_source.metadata_asset,
        )?;
        let metadata: ReleaseMetadata =
            serde_json::from_slice(&self.get(&metadata_asset.browser_download_url, 16 * 1024)?)?;
        if metadata.schema_version != 1
            || metadata.host != "znet-sink"
            || metadata.plugin_id != registration.id
        {
            return Err("release metadata identity mismatch".into());
        }
        let package_asset = asset(registration, release, &metadata.asset)?;
        if package_asset.size > MAX_PACKAGE_BYTES as u64 {
            return Err("package exceeds limit".into());
        }
        let bytes = self.get(&package_asset.browser_download_url, MAX_PACKAGE_BYTES)?;
        validate_download(&bytes, &metadata, registration)?;
        Ok(bytes)
    }
}
pub fn validate_download(
    bytes: &[u8],
    metadata: &ReleaseMetadata,
    registration: &Registration,
) -> Result<()> {
    if metadata.schema_version != 1
        || metadata.host != "znet-sink"
        || metadata.plugin_id != registration.id
        || sha256(bytes) != metadata.sha256
    {
        return Err("release identity or digest mismatch".into());
    }
    let package = verify(bytes, registration)?;
    if package.version != metadata.version {
        return Err("signed package differs from release version".into());
    }
    Ok(())
}
fn asset<'a>(registration: &Registration, release: &'a Release, name: &str) -> Result<&'a Asset> {
    let matches: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
    if matches.len() != 1 {
        return Err("release asset missing or ambiguous".into());
    }
    let selected = matches[0];
    let url = Url::parse(&selected.browser_download_url)?;
    let prefix = format!("/{}/releases/download/", registration.repository_path()?);
    if url.host_str() != Some("github.com") || !url.path().starts_with(&prefix) {
        return Err("asset belongs to another repository".into());
    }
    Ok(selected)
}
fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && (host == "github.com"
            || host == "api.github.com"
            || host.ends_with(".githubusercontent.com"))
}
