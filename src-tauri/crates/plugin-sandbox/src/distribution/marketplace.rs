//! Central registrations and publisher-owned release manifest contracts.
use super::{
    directory::{Directory, Publisher, Registration, ReleaseSource},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceRelease {
    pub version: String,
    pub channel: String,
    pub published_at: String,
    pub notes_url: String,
    pub surfaces: Vec<String>,
    pub capabilities: Vec<String>,
    pub artifacts: Vec<MarketplaceArtifact>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceArtifact {
    pub os: String,
    pub arch: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub signature: Option<ArtifactSignature>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSignature {
    pub algorithm: String,
    pub value: String,
}
mod registration;
mod validation;
pub(super) use validation::{validate_registration, validate_release};

#[derive(Deserialize)]
pub(crate) struct HostVersion {
    pub min: String,
    pub max_exclusive: Option<String>,
}
impl HostVersion {
    pub(crate) fn supports(&self, current: &semver::Version) -> Result<bool> {
        let min = semver::Version::parse(&self.min)?;
        let max = self
            .max_exclusive
            .as_deref()
            .map(semver::Version::parse)
            .transpose()?;
        if max.as_ref().is_some_and(|max| max <= &min) {
            return Err("invalid marketplace host version range".into());
        }
        Ok(current >= &min && max.as_ref().is_none_or(|max| current < max))
    }
}
