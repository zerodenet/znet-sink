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
            return Err("release version differs from marketplace metadata".into());
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
        if artifact.size > MAX_PACKAGE_BYTES as u64 {
            return Err("package exceeds the ZNet Sink size limit".into());
        }
        let bytes = self.get_package(&artifact.url, artifact.size as usize, &artifact.sha256)?;
        if bytes.len() as u64 != artifact.size {
            return Err("downloaded package size differs from marketplace metadata".into());
        }
        let signature = artifact
            .signature
            .as_ref()
            .ok_or("marketplace artifact has no signature")?;
        let envelope: super::super::package::Envelope = serde_json::from_slice(&bytes)?;
        if envelope.signature != signature.value {
            return Err("package signature differs from marketplace metadata".into());
        }
        let metadata = ReleaseMetadata {
            schema_version: 1,
            host: "znet-sink".into(),
            plugin_id: registration.id.clone(),
            version: record.version.clone(),
            asset: artifact
                .url
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned(),
            sha256: artifact.sha256.clone(),
        };
        let mut boundary = registration.clone();
        boundary.surfaces = record.surfaces.clone();
        boundary.capabilities = record.capabilities.clone();
        validate_download(&bytes, &metadata, &boundary)?;
        Ok(bytes)
    }
}
