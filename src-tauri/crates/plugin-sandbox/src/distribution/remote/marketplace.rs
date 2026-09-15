use super::*;

impl Remote<'_> {
    pub(super) fn download_target(
        &self,
        registration: &Registration,
        release: &Release,
        record: &super::super::marketplace::MarketplaceRelease,
    ) -> Result<Vec<u8>> {
        super::super::marketplace::validate_release(registration, record)?;
        if release.tag_name != format!("v{}", record.version) {
            return Err("release version differs from publisher manifest".into());
        }
        let native = crate::contract::Target::native_desktop()?;
        let (os, arch) = marketplace_platform(&native)?;
        let artifacts: Vec<_> = record
            .artifacts
            .iter()
            .filter(|artifact| {
                (artifact.os == "any" || artifact.os == os)
                    && (artifact.arch == "any" || artifact.arch == arch)
            })
            .collect();
        if artifacts.len() != 1 {
            return Err("release does not uniquely select a native package".into());
        }
        let artifact = artifacts[0];
        if artifact.size > MAX_PACKAGE_BYTES as u64
            || artifact.sha256.len() != 64
            || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid publisher package metadata".into());
        }
        let matching_assets: Vec<_> = release
            .assets
            .iter()
            .filter(|asset| {
                asset.browser_download_url == artifact.url && asset.size == artifact.size
            })
            .collect();
        if matching_assets.len() != 1 {
            return Err("selected release differs from publisher manifest".into());
        }
        let bytes = self.get(&artifact.url, MAX_PACKAGE_BYTES)?;
        let metadata = ReleaseMetadata {
            schema_version: 1,
            host: "znet-sink".into(),
            plugin_id: registration.id.clone(),
            version: record.version.clone(),
            asset: matching_assets[0].name.clone(),
            sha256: artifact.sha256.clone(),
        };
        if bytes.len() as u64 != artifact.size {
            return Err("downloaded package size differs from publisher manifest".into());
        }
        if let Some(signature) = &artifact.signature {
            let envelope: super::super::package::Envelope = serde_json::from_slice(&bytes)?;
            if envelope.signature != signature.value {
                return Err("package signature differs from publisher manifest".into());
            }
        }
        let mut boundary = registration.clone();
        boundary.surfaces = record.surfaces.clone();
        boundary.capabilities = record.capabilities.clone();
        validate_download(&bytes, &metadata, &boundary)?;
        Ok(bytes)
    }
}

impl Remote<'_> {
    /// Read the selected repository release manifest, never the central release feed.
    pub(super) fn unified_target(
        &self,
        bytes: &[u8],
        registration: &Registration,
        github: &Release,
    ) -> Result<super::super::marketplace::MarketplaceRelease> {
        use super::super::{
            directory::Publisher,
            marketplace::{HostVersion, MarketplaceArtifact, MarketplaceRelease},
        };
        #[derive(Deserialize)]
        struct Document {
            schema_version: u32,
            product_id: String,
            repository: String,
            publisher: Publisher,
            source: Source,
            release: Published,
        }
        #[derive(Deserialize)]
        struct Source {
            tag: String,
            commit: String,
        }
        #[derive(Deserialize)]
        struct Published {
            version: String,
            channel: String,
            published_at: String,
            notes_url: String,
            targets: Vec<Target>,
        }
        #[derive(Deserialize)]
        struct Target {
            host: String,
            package_id: String,
            host_version: HostVersion,
            surfaces: Vec<String>,
            capabilities: Vec<String>,
            artifacts: Vec<MarketplaceArtifact>,
        }
        let document: Document = serde_json::from_slice(bytes)?;
        if document.schema_version != 1
            || registration
                .product_id
                .as_ref()
                .is_some_and(|id| id != &document.product_id)
            || document.repository != registration.repository
            || document.publisher.id != registration.publisher.id
            || document.publisher.public_key != registration.publisher.public_key
            || document.source.tag != github.tag_name
            || document.source.commit.len() != 40
            || !document
                .source
                .commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || document.source.tag != format!("v{}", document.release.version)
            || document.release.targets.len() > 2
            || github.draft
            || github.published_at.is_none()
        {
            return Err("release metadata identity mismatch".into());
        }
        let mut publisher_identity = registration.clone();
        publisher_identity.product_id = Some(document.product_id);
        publisher_identity.validate()?;
        let mut targets = document
            .release
            .targets
            .into_iter()
            .filter(|target| target.host == "znet-sink");
        let target = targets.next().ok_or("release has no ZNet Sink target")?;
        if targets.next().is_some()
            || target.package_id != registration.id
            || !target
                .host_version
                .supports(&semver::Version::parse(&self.host_version)?)?
        {
            return Err("release target is incompatible or ambiguous".into());
        }
        // Publisher manifests must declare a signature for every artifact.
        if target
            .artifacts
            .iter()
            .any(|artifact| artifact.signature.is_none())
        {
            return Err("release manifest has no artifact signature".into());
        }
        let target = MarketplaceRelease {
            version: document.release.version,
            channel: document.release.channel,
            published_at: document.release.published_at,
            notes_url: document.release.notes_url,
            surfaces: target.surfaces,
            capabilities: target.capabilities,
            artifacts: target.artifacts,
        };
        super::super::marketplace::validate_release(registration, &target)?;
        Ok(target)
    }
}
