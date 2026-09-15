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
pub const MARKETPLACE_API_URL: &str = "https://plugins.zerodenet.org/api/plugins";
type Fetch<'a> = dyn Fn(&str, usize) -> Result<Vec<u8>> + 'a;
pub struct Remote<'a> {
    fetch: Option<Box<Fetch<'a>>>,
    client: Client,
    host_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    #[serde(default)]
    pub channel: Option<String>,
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
            host_version: env!("CARGO_PKG_VERSION").into(),
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
    /// The desktop host supplies its product version, independently of this crate's version.
    pub fn for_host(mut self, version: &str) -> Result<Self> {
        semver::Version::parse(version)?;
        self.host_version = version.into();
        Ok(self)
    }
    /// Complete trust registrations, including products without an online release.
    pub fn registration_directory(&self) -> Result<Directory> {
        self.get(&format!("{MARKETPLACE_API_URL}.json"), 1024 * 1024)
            .and_then(|bytes| Directory::parse_marketplace_registrations(&bytes))
            .or_else(|_| Directory::parse(&self.get(DIRECTORY_URL, 1024 * 1024)?))
    }
    /// Discovery uses registrations only. The publisher repository owns releases.
    pub fn directory(&self) -> Result<Directory> {
        self.registration_directory()
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
        if release.draft || release.published_at.is_none() {
            return Err("release is not published".into());
        }
        let metadata_asset = asset(
            registration,
            release,
            &registration.release_source.metadata_asset,
        )?;
        let metadata_bytes = self.get(&metadata_asset.browser_download_url, 512 * 1024)?;
        let metadata: ReleaseMetadata = match serde_json::from_slice(&metadata_bytes) {
            Ok(metadata) => metadata,
            Err(_) => {
                let target = self.unified_target(&metadata_bytes, registration, release)?;
                return self.download_target(registration, release, &target);
            }
        };
        if metadata.schema_version != 1
            || metadata.host != "znet-sink"
            || metadata.plugin_id != registration.id
            || release.tag_name != format!("v{}", metadata.version)
        {
            return Err("release metadata identity mismatch".into());
        }
        let package_asset = asset(registration, release, &metadata.asset)?;
        if package_asset.size > MAX_PACKAGE_BYTES as u64 {
            return Err("package exceeds limit".into());
        }
        let bytes = self.get(&package_asset.browser_download_url, MAX_PACKAGE_BYTES)?;
        if bytes.len() as u64 != package_asset.size {
            return Err("downloaded package size differs from release".into());
        }
        validate_download(&bytes, &metadata, registration)?;
        Ok(bytes)
    }
}

pub(super) fn marketplace_platform(
    target: &crate::contract::Target,
) -> Result<(&'static str, &'static str)> {
    use crate::contract::{Arch, Os};
    let os = match target.os {
        Os::Macos => "darwin",
        Os::Windows => "windows",
        Os::Linux => "linux",
        Os::Android => "android",
        Os::Ios => "ios",
    };
    let arch = match target.arch {
        Arch::X86_64 => "amd64",
        Arch::Aarch64 => "arm64",
        Arch::Armv7 => return Err("marketplace does not support this native architecture".into()),
    };
    Ok((os, arch))
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
    let prefix = format!(
        "/{}/releases/download/{}/",
        registration.repository_path()?,
        release.tag_name
    );
    if !allowed(&url)
        || url.host_str() != Some("github.com")
        || !url.path().starts_with(&prefix)
        || url.path()[prefix.len()..].contains('/')
        || url.query().is_some()
        || url.fragment().is_some()
    {
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
        && (host == "plugins.zerodenet.org"
            || host == "github.com"
            || host == "api.github.com"
            || host.ends_with(".githubusercontent.com"))
}

mod marketplace;
