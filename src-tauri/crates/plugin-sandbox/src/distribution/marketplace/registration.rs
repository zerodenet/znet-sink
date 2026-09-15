//! Discovery and admission read registered targets, independently of release availability.
use super::*;

#[derive(Deserialize)]
struct Snapshot {
    schema_version: u32,
    snapshot_version: String,
    sources: Sources,
    products: Vec<Product>,
}
#[derive(Deserialize)]
struct Sources {
    registry_schema: u32,
}
#[derive(Deserialize)]
struct Product {
    id: String,
    repository: String,
    publisher: Publisher,
    name: String,
    description: String,
    license: String,
    maintainers: Vec<String>,
    homepage: Option<String>,
    documentation: Option<String>,
    security: Option<String>,
    release_source: ReleaseSource,
    #[serde(default)]
    withdrawn: bool,
    targets: Vec<Target>,
}
#[derive(Deserialize)]
struct Target {
    host: String,
    package_id: String,
    surfaces: Vec<String>,
    capabilities: Vec<String>,
    #[serde(default)]
    releases: Vec<SnapshotRelease>,
}
#[derive(Deserialize)]
struct SnapshotRelease {
    #[serde(flatten)]
    release: MarketplaceRelease,
    host_version: HostVersion,
}
impl Directory {
    pub fn parse_marketplace(bytes: &[u8], host_version: &str) -> Result<Self> {
        if bytes.len() > 1024 * 1024 {
            return Err("directory exceeds limit".into());
        }
        let snapshot: Snapshot = serde_json::from_slice(bytes)?;
        let digest = snapshot
            .snapshot_version
            .strip_prefix("sha256:")
            .unwrap_or("");
        if snapshot.schema_version != 1
            || snapshot.sources.registry_schema != 3
            || snapshot.products.len() > 1000
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("unsupported marketplace registry snapshot".into());
        }
        let mut plugins = Vec::new();
        let current_host = semver::Version::parse(host_version)?;
        let mut products = BTreeSet::new();
        let mut packages = BTreeSet::new();
        for product in snapshot.products {
            if !products.insert(product.id.clone())
                || product.targets.is_empty()
                || product.targets.len() > 2
            {
                return Err("invalid marketplace registry product".into());
            }
            if product.withdrawn {
                continue;
            }
            let mut hosts = BTreeSet::new();
            let mut selected = None;
            for target in product.targets {
                if !matches!(target.host.as_str(), "znet-sink" | "zboard")
                    || !hosts.insert(target.host.clone())
                {
                    return Err("invalid marketplace registry target".into());
                }
                if target.host == "znet-sink" {
                    selected = Some(target);
                }
            }
            let Some(target) = selected else {
                continue;
            };
            if !packages.insert(target.package_id.clone()) {
                return Err("duplicate registration".into());
            }
            let mut registration = Registration {
                product_id: Some(product.id),
                id: target.package_id,
                repository: product.repository,
                publisher: product.publisher,
                name: product.name,
                description: product.description,
                license: product.license,
                maintainers: product.maintainers,
                homepage: product.homepage,
                documentation: product.documentation,
                security: product.security,
                release_source: product.release_source,
                surfaces: target.surfaces,
                capabilities: target.capabilities,
                releases: Vec::new(),
            };
            registration.validate()?;
            for snapshot_release in target.releases {
                validate_release(&registration, &snapshot_release.release)?;
                if snapshot_release.host_version.supports(&current_host)? {
                    registration.releases.push(snapshot_release.release);
                }
            }
            registration.releases.sort_by(|left, right| {
                semver::Version::parse(&right.version)
                    .ok()
                    .cmp(&semver::Version::parse(&left.version).ok())
            });
            plugins.push(registration);
        }
        Ok(Self {
            schema_version: 2,
            snapshot_version: Some(snapshot.snapshot_version),
            host: "znet-sink".into(),
            plugins,
        })
    }

    pub fn parse_marketplace_registrations(bytes: &[u8]) -> Result<Self> {
        Self::parse_marketplace(bytes, env!("CARGO_PKG_VERSION"))
    }
}
