use super::marketplace::MarketplaceRelease;
use super::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Directory {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_version: Option<String>,
    pub schema_version: u32,
    pub host: String,
    pub plugins: Vec<Registration>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publisher {
    pub id: String,
    pub public_key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub metadata_asset: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    pub id: String,
    pub repository: String,
    pub publisher: Publisher,
    pub name: String,
    pub description: String,
    pub license: String,
    pub maintainers: Vec<String>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    pub security: Option<String>,
    pub release_source: ReleaseSource,
    pub surfaces: Vec<String>,
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<MarketplaceRelease>,
}
impl Directory {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 1024 * 1024 {
            return Err("directory exceeds limit".into());
        }
        let directory: Self = serde_json::from_slice(bytes)?;
        if directory.schema_version != 2
            || directory.host != "znet-sink"
            || directory.plugins.len() > 1000
        {
            return Err("unsupported directory".into());
        }
        let mut ids = BTreeSet::new();
        for entry in &directory.plugins {
            if !ids.insert(&entry.id) {
                return Err("duplicate registration".into());
            }
            entry.validate()?;
        }
        Ok(directory)
    }
    pub fn find(&self, id: &str) -> Result<&Registration> {
        self.plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "plugin is not registered".into())
    }
}
impl Registration {
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || self.publisher.id.is_empty()
        {
            return Err("invalid publisher or plugin identity".into());
        }
        if STANDARD.decode(&self.publisher.public_key)?.len() != 32 {
            return Err("invalid publisher key".into());
        }
        self.repository_path()?;
        if self.release_source.kind != "github-releases"
            || self.release_source.metadata_asset.is_empty()
            || self.release_source.metadata_asset.len() > 128
            || self.release_source.metadata_asset.contains(['/', '\\'])
        {
            return Err("unsupported release source".into());
        }
        super::marketplace::validate_registration(self)?;
        Ok(())
    }
    pub fn repository_path(&self) -> Result<String> {
        let url = reqwest::Url::parse(&self.repository)?;
        let segments: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || segments.len() != 2
            || segments.iter().any(|s| {
                s.is_empty()
                    || *s == "."
                    || *s == ".."
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err("repository must be an HTTPS GitHub owner/repository".into());
        }
        Ok(segments.join("/"))
    }
}
