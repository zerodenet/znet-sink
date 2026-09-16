use std::collections::BTreeSet;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidManifest,
    WrongHost,
    IncompatibleDevice,
    IncompatibleVersion,
    UnsupportedIsolation,
    AdmissionDenied,
    DigestMismatch,
    PermissionDenied,
    Disabled,
    Busy,
    Revoked,
    Expired,
    Deadline,
    Cancelled,
    BudgetExceeded,
    GuestException,
    InvalidOutput,
    HostFailure,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Macos,
    Windows,
    Linux,
    Android,
    Ios,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    X86_64,
    Aarch64,
    Armv7,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    Desktop,
    Phone,
    Tablet,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub os: Os,
    pub arch: Arch,
    pub device: DeviceClass,
}

/// Explicit unrestricted device declaration, or an exact list of device tuples.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Targets {
    Any(AnyTarget),
    Only(Vec<Target>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnyTarget {
    #[serde(rename = "any")]
    Any,
}
impl Targets {
    fn valid(&self) -> bool {
        match self {
            Self::Any(_) => true,
            Self::Only(targets) => {
                !targets.is_empty() && targets.len() <= 16 && targets.iter().all(Target::valid)
            }
        }
    }
    fn matches(&self, target: &Target) -> bool {
        match self {
            Self::Any(_) => true,
            Self::Only(targets) => targets.contains(target),
        }
    }
}

impl Target {
    pub fn native_desktop() -> Result<Self, Error> {
        let os = match std::env::consts::OS {
            "macos" => Os::Macos,
            "windows" => Os::Windows,
            "linux" => Os::Linux,
            _ => return Err(Error::IncompatibleDevice),
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => Arch::X86_64,
            "aarch64" => Arch::Aarch64,
            _ => return Err(Error::IncompatibleDevice),
        };
        Ok(Self {
            os,
            arch,
            device: DeviceClass::Desktop,
        })
    }
    fn valid(&self) -> bool {
        let architecture_valid = match self.os {
            Os::Android => true,
            Os::Ios => self.arch == Arch::Aarch64,
            _ => matches!(self.arch, Arch::X86_64 | Arch::Aarch64),
        };
        architecture_valid
            && matches!(
                (&self.os, &self.device),
                (Os::Macos | Os::Windows | Os::Linux, DeviceClass::Desktop)
                    | (
                        Os::Android | Os::Ios,
                        DeviceClass::Phone | DeviceClass::Tablet
                    )
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "plugin.self.read")]
    SelfRead,
    #[serde(rename = "records.summary.read")]
    RecordsSummaryRead,
    #[serde(rename = "network.get")]
    NetworkGet,
    #[serde(rename = "network.request")]
    NetworkRequest,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub capability: Capability,
    pub scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    Vm,
    Process,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    pub timeout_ms: u64,
    pub max_calls: u32,
    pub output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 32 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            timeout_ms: 2_000,
            max_calls: 32,
            output_bytes: 64 * 1024,
        }
    }
}
impl Limits {
    fn valid(&self) -> bool {
        (2 * 1024 * 1024..=32 * 1024 * 1024).contains(&self.memory_bytes)
            && (64 * 1024..=512 * 1024).contains(&self.stack_bytes)
            && (1..=2_000).contains(&self.timeout_ms)
            && (1..=32).contains(&self.max_calls)
            && (1..=64 * 1024).contains(&self.output_bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub host: String,
    pub plugin_id: String,
    pub component_id: String,
    pub version: String,
    pub requires_host: String,
    pub api_version: u32,
    pub runtime: String,
    pub minimum_isolation: Isolation,
    pub targets: Targets,
    pub required: Vec<Request>,
    pub optional: Vec<Request>,
    pub source_sha256: String,
    pub limits: Limits,
}

/// Local source identity only. A digest is NOT publisher authentication.
pub struct Component {
    pub(crate) manifest: Manifest,
    pub(crate) source: String,
    pub(crate) digest: String,
}
impl Component {
    pub fn load(manifest_bytes: &[u8], source: &str) -> Result<Self, Error> {
        if manifest_bytes.len() > MAX_MANIFEST_BYTES || source.len() > MAX_SOURCE_BYTES {
            return Err(Error::BudgetExceeded);
        }
        let manifest: Manifest =
            serde_json::from_slice(manifest_bytes).map_err(|_| Error::InvalidManifest)?;
        if manifest.host != "znet-sink" {
            return Err(Error::WrongHost);
        }
        if manifest.schema_version != 1
            || manifest.api_version != 1
            || manifest.runtime != "javascript-v1"
            || !identifier(&manifest.plugin_id)
            || !identifier(&manifest.component_id)
            || Version::parse(&manifest.version).is_err()
            || VersionReq::parse(&manifest.requires_host).is_err()
            || !manifest.targets.valid()
            || !manifest.limits.valid()
            || manifest.required.len() + manifest.optional.len() > 16
        {
            return Err(Error::InvalidManifest);
        }
        let mut seen = BTreeSet::new();
        for request in manifest.required.iter().chain(&manifest.optional) {
            let valid_scope = match request.capability {
                Capability::SelfRead => request.scope == "self",
                Capability::NetworkGet | Capability::NetworkRequest => {
                    znet_client_capabilities::network::origin(&request.scope)
                        .is_ok_and(|origin| origin == request.scope)
                }
                Capability::RecordsSummaryRead => {
                    request.scope.starts_with("selection:") && identifier(&request.scope[10..])
                }
            };
            if !valid_scope || !seen.insert(request) {
                return Err(Error::InvalidManifest);
            }
        }
        if manifest.source_sha256 != sha256(source.as_bytes()) {
            return Err(Error::DigestMismatch);
        }
        let mut hasher = Sha256::new();
        hasher.update(manifest_bytes);
        hasher.update([0]);
        hasher.update(source.as_bytes());
        Ok(Self {
            manifest,
            source: source.into(),
            digest: format!("{:x}", hasher.finalize()),
        })
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn compatible(
        &self,
        target: &Target,
        host_version: &str,
        isolation: Isolation,
    ) -> Result<(), Error> {
        self.compatible_device(target, host_version)?;
        if self.manifest.minimum_isolation == Isolation::Process && isolation != Isolation::Process
        {
            return Err(Error::UnsupportedIsolation);
        }
        Ok(())
    }
    pub fn compatible_device(&self, target: &Target, host_version: &str) -> Result<(), Error> {
        if !target.valid() || !self.manifest.targets.matches(target) {
            return Err(Error::IncompatibleDevice);
        }
        let version = Version::parse(host_version).map_err(|_| Error::IncompatibleVersion)?;
        let requirement = VersionReq::parse(&self.manifest.requires_host)
            .map_err(|_| Error::IncompatibleVersion)?;
        // Product dev/rc builds implement the API of their base release. SemVer
        // deliberately excludes prereleases from ordinary ranges, so first keep
        // support for an explicit prerelease requirement, then compare the base
        // version for normal host compatibility ranges.
        let base = host_compatibility_version(&version);
        if !requirement.matches(&version) && !requirement.matches(&base) {
            return Err(Error::IncompatibleVersion);
        }
        Ok(())
    }
}

pub(crate) fn host_compatibility_version(version: &Version) -> Version {
    Version::new(version.major, version.minor, version.patch)
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
