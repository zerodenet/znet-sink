use super::{
    directory::{Directory, Registration},
    package::verify,
    Result, MAX_PACKAGE_BYTES,
};
use crate::contract::sha256;
use reqwest::{blocking::Client, Url};
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};

/// Current public metadata endpoint. The future custom domain can replace this
/// constant without changing the publisher-owned GitHub package path.
pub const MARKETPLACE_API_URL: &str = "https://zerodenet.github.io/plugins/api/plugins.json";

type Fetch<'a> = dyn Fn(&str, usize) -> Result<Vec<u8>> + 'a;
type PackageFetch<'a> = dyn Fn(&str, usize, &str) -> Result<Vec<u8>> + 'a;

pub struct Remote<'a> {
    fetch: Option<Box<Fetch<'a>>>,
    package_fetch: Option<Box<PackageFetch<'a>>>,
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
            package_fetch: None,
        })
    }

    /// Route bounded metadata reads through the host's capability executor.
    pub fn with_fetch(fetch: impl Fn(&str, usize) -> Result<Vec<u8>> + 'a) -> Result<Self> {
        let mut remote = Self::new()?;
        remote.fetch = Some(Box::new(fetch));
        Ok(remote)
    }

    /// Let the desktop host persist and resume only publisher package bodies.
    pub fn with_package_fetch(
        mut self,
        fetch: impl Fn(&str, usize, &str) -> Result<Vec<u8>> + 'a,
    ) -> Self {
        self.package_fetch = Some(Box::new(fetch));
        self
    }

    pub fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        let url = Url::parse(url)?;
        if !allowed(&url) {
            return Err("untrusted marketplace URL".into());
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

    fn get_package(&self, url: &str, limit: usize, expected_sha256: &str) -> Result<Vec<u8>> {
        let url = Url::parse(url)?;
        if url.host_str() != Some("github.com") || !allowed(&url) {
            return Err("package must use the publisher GitHub release URL".into());
        }
        if let Some(fetch) = &self.package_fetch {
            return fetch(url.as_str(), limit, expected_sha256);
        }
        self.get(url.as_str(), limit)
    }

    pub fn for_host(mut self, version: &str) -> Result<Self> {
        semver::Version::parse(version)?;
        self.host_version = version.into();
        Ok(self)
    }

    /// The marketplace owns registrations, releases, compatibility and hashes.
    pub fn registration_directory(&self) -> Result<Directory> {
        Directory::parse_marketplace(
            &self.get(MARKETPLACE_API_URL, 4 * 1024 * 1024)?,
            &self.host_version,
        )
    }

    pub fn directory(&self) -> Result<Directory> {
        self.registration_directory()
    }

    pub fn releases(&self, registration: &Registration) -> Result<Vec<Release>> {
        registration
            .releases
            .iter()
            .map(|record| {
                Ok(Release {
                    channel: Some(record.channel.clone()),
                    tag_name: format!("v{}", record.version),
                    name: Some(format!("{} {}", registration.name, record.version)),
                    body: None,
                    published_at: Some(record.published_at.clone()),
                    html_url: record.notes_url.clone(),
                    draft: false,
                    prerelease: record.channel != "stable",
                    assets: record
                        .artifacts
                        .iter()
                        .map(|artifact| {
                            let url = Url::parse(&artifact.url)?;
                            let name = url
                                .path_segments()
                                .and_then(|mut segments| segments.next_back())
                                .filter(|name| !name.is_empty())
                                .ok_or("invalid marketplace artifact name")?;
                            Ok(Asset {
                                name: name.to_owned(),
                                browser_download_url: artifact.url.clone(),
                                size: artifact.size,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect()
    }

    pub fn release(&self, registration: &Registration, tag: &str) -> Result<Release> {
        if tag.is_empty() || tag.len() > 128 {
            return Err("invalid release tag".into());
        }
        self.releases(registration)?
            .into_iter()
            .find(|release| release.tag_name == tag)
            .ok_or_else(|| "release is not present in the marketplace snapshot".into())
    }

    pub fn download(&self, registration: &Registration, release: &Release) -> Result<Vec<u8>> {
        let version = release
            .tag_name
            .strip_prefix('v')
            .ok_or("invalid release tag")?;
        let records: Vec<_> = registration
            .releases
            .iter()
            .filter(|record| record.version == version)
            .collect();
        if records.len() != 1 {
            return Err("release is missing or ambiguous in marketplace snapshot".into());
        }
        self.download_target(registration, release, records[0])
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

fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or("");
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && (host == "zerodenet.github.io"
            || host == "github.com"
            || host == "api.github.com"
            || host.ends_with(".githubusercontent.com"))
}

mod marketplace;
