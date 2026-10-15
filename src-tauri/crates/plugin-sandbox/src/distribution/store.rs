use super::{
    directory::Registration,
    package::{verify, VerifiedPackage},
    Result,
};
use crate::contract::sha256;
use crate::contract::Target;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
const MAX_STORE: usize = 1024 * 1024;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    #[serde(default)]
    epoch: u64,
    #[serde(default)]
    generations: BTreeMap<String, u64>,
    packages: BTreeMap<String, Installed>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    #[serde(default)]
    generation: u64,
    current: StoredPackage,
    previous: Option<StoredPackage>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum StoredPackage {
    /// Compatibility with the original JSON-envelope installation store.
    Inline(String),
    /// Application packages and newly installed legacy packages are kept as exact binary blobs.
    Blob { sha256: String },
}
/// Exact verified installation, including cooperative store history. No source
/// text is retained in an uninstall plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UninstallReview {
    pub plugin_id: String,
    pub store_digest: String,
    pub store_epoch: u64,
    pub installation_generation: u64,
    pub package_digest: String,
    pub current_sha256: String,
    pub previous_sha256: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UninstallReceipt {
    pub review: UninstallReview,
    pub after_digest: String,
    pub after_epoch: u64,
    /// Immutable content-addressed blobs are deliberately retained. This is a
    /// registration-removal receipt, never a physical-blob cleanup receipt.
    pub blobs_retained: bool,
}
struct StoreLock(File);
impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}
/// Atomic metadata plus content-addressed package blobs. Packages are verified
/// directly from bytes and are never extracted into executable host paths.
pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        reject_symlink(&root)?;
        Ok(Self { root })
    }
    /// Open an existing store without creating business directories.
    pub fn open_existing(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        reject_symlink(&root)?;
        if !fs::metadata(&root)?.is_dir() {
            return Err("invalid installation root".into());
        }
        Ok(Self { root })
    }
    pub fn review_uninstall(
        &self,
        registration: &Registration,
    ) -> Result<(UninstallReview, VerifiedPackage)> {
        let _lock = self.lock()?;
        let state = self.read()?;
        let installed = state
            .packages
            .get(&registration.id)
            .ok_or("plugin is not installed")?;
        let current = installed.current.read(&self.root)?;
        let verified = verify(&current, registration)?;
        let review = UninstallReview {
            plugin_id: registration.id.clone(),
            store_digest: sha256(&serde_json::to_vec(&state)?),
            store_epoch: state.epoch,
            installation_generation: installed.generation,
            package_digest: verified.digest.clone(),
            current_sha256: sha256(&current),
            previous_sha256: installed
                .previous
                .as_ref()
                .map(|p| p.read(&self.root).map(|b| sha256(&b)))
                .transpose()?,
        };
        Ok((review, verified))
    }
    /// Caller has fixed a signed registration and original material. There is
    /// no missing-row success and no second attempt after an uncertain result.
    pub fn uninstall_checked(
        &self,
        registration: &Registration,
        expected: &UninstallReview,
    ) -> Result<UninstallReceipt> {
        let _lock = self.lock()?;
        let mut state = self.read()?;
        let installed = state
            .packages
            .get(&registration.id)
            .ok_or("plugin is not installed")?;
        let current = installed.current.read(&self.root)?;
        let verified = verify(&current, registration)?;
        let actual = UninstallReview {
            plugin_id: registration.id.clone(),
            store_digest: sha256(&serde_json::to_vec(&state)?),
            store_epoch: state.epoch,
            installation_generation: installed.generation,
            package_digest: verified.digest,
            current_sha256: sha256(&current),
            previous_sha256: installed
                .previous
                .as_ref()
                .map(|p| p.read(&self.root).map(|b| sha256(&b)))
                .transpose()?,
        };
        if &actual != expected {
            return Err("installation changed after review".into());
        }
        let generation = installed.generation;
        state.packages.remove(&registration.id);
        advance_generation(&mut state, &registration.id, generation)?;
        self.save(&state)?;
        let after = self.read()?;
        Ok(UninstallReceipt {
            review: actual,
            after_digest: sha256(&serde_json::to_vec(&after)?),
            after_epoch: after.epoch,
            blobs_retained: true,
        })
    }
    fn lock(&self) -> Result<StoreLock> {
        let path = self.root.join("install.lock");
        reject_symlink(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err("nonregular installation lock rejected".into());
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        // fs2 reports Windows contention as ERROR_LOCK_VIOLATION, whose
        // ErrorKind is not necessarily WouldBlock. Compare its documented
        // platform error as well; unrelated IO errors still fail immediately.
        let contended = fs2::lock_contended_error().raw_os_error();
        loop {
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(StoreLock(file)),
                Err(error)
                    if (error.kind() == std::io::ErrorKind::WouldBlock
                        || (contended.is_some() && error.raw_os_error() == contended))
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || (contended.is_some() && error.raw_os_error() == contended) =>
                {
                    return Err("installation store busy".into())
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn read(&self) -> Result<State> {
        let path = self.root.join("installed.json");
        reject_regular_if_present(&path)?;
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take((MAX_STORE + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > MAX_STORE {
            return Err("installation store exceeds limit".into());
        }
        let state: State = serde_json::from_slice(&bytes)?;
        if state.packages.len() > 16
            || state.generations.len() > 4096
            || state
                .generations
                .values()
                .any(|generation| *generation == 0)
            || state
                .packages
                .iter()
                .any(|(id, p)| match state.generations.get(id) {
                    Some(generation) => *generation == 0 || *generation != p.generation,
                    None => p.generation != 0,
                })
        {
            return Err("too many installed packages".into());
        }
        Ok(state)
    }
    fn save(&self, state: &State) -> Result<()> {
        let mut value = serde_json::to_value(state)?;
        value["epoch"] = serde_json::Value::from(
            state
                .epoch
                .checked_add(1)
                .ok_or("installation epoch exhausted")?,
        );
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() > MAX_STORE || state.packages.len() > 16 || state.generations.len() > 4096 {
            return Err("installation capacity exceeded".into());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.root.join("installed.json"))?;
        #[cfg(unix)]
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub fn install(
        &self,
        bytes: &[u8],
        registration: &Registration,
        target: &Target,
        host_version: &str,
    ) -> Result<VerifiedPackage> {
        let verified = verify(bytes, registration)?;
        verified.compatible(target, host_version)?;
        let _lock = self.lock()?;
        let mut state = self.read()?;
        let previous = if let Some(old) = state.packages.get(&registration.id) {
            let old_bytes = old.current.read(&self.root)?;
            let old_verified = verify(&old_bytes, registration)?;
            if semver::Version::parse(&verified.version)?
                <= semver::Version::parse(&old_verified.version)?
            {
                return Err("install requires a newer version; use explicit rollback".into());
            }
            Some(old.current.clone())
        } else {
            None
        };
        let (current, created) = self.store_blob(bytes)?;
        let generation = advance_generation(&mut state, &registration.id, 0)?;
        state.packages.insert(
            registration.id.clone(),
            Installed {
                generation,
                current,
                previous,
            },
        );
        if let Err(error) = self.save(&state) {
            if created {
                let _ = fs::remove_file(self.blob_path(&sha256(bytes))?);
            }
            return Err(error);
        }
        self.cleanup_unreferenced_blobs(&state);
        Ok(verified)
    }
    pub fn current(&self, registration: &Registration) -> Result<VerifiedPackage> {
        let _lock = self.lock()?;
        let state = self.read()?;
        let entry = state
            .packages
            .get(&registration.id)
            .ok_or("plugin is not installed")?;
        verify(&entry.current.read(&self.root)?, registration)
    }
    pub fn list(&self) -> Result<Vec<String>> {
        let _lock = self.lock()?;
        Ok(self.read()?.packages.into_keys().collect())
    }
    pub fn uninstall(&self, id: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut state = self.read()?;
        let removed = state.packages.remove(id).ok_or("plugin is not installed")?;
        advance_generation(&mut state, id, removed.generation)?;
        self.save(&state)?;
        self.cleanup_unreferenced_blobs(&state);
        Ok(())
    }
    pub fn rollback(
        &self,
        registration: &Registration,
        target: &Target,
        host_version: &str,
    ) -> Result<VerifiedPackage> {
        let _lock = self.lock()?;
        let mut state = self.read()?;
        let entry = state
            .packages
            .get_mut(&registration.id)
            .ok_or("plugin is not installed")?;
        let previous = entry.previous.as_ref().ok_or("no rollback version")?;
        let verified = verify(&previous.read(&self.root)?, registration)?;
        verified.compatible(target, host_version)?;
        let previous = entry.previous.take().unwrap();
        entry.previous = Some(std::mem::replace(&mut entry.current, previous));
        let previous_generation = entry.generation;
        let generation = advance_generation(&mut state, &registration.id, previous_generation)?;
        state
            .packages
            .get_mut(&registration.id)
            .ok_or("plugin is not installed")?
            .generation = generation;
        self.save(&state)?;
        Ok(verified)
    }
}

impl Store {
    fn blob_dir(&self) -> Result<PathBuf> {
        let path = self.root.join("packages");
        reject_symlink(&path)?;
        fs::create_dir_all(&path)?;
        reject_symlink(&path)?;
        Ok(path)
    }

    fn blob_path(&self, digest: &str) -> Result<PathBuf> {
        validate_digest(digest)?;
        Ok(self.blob_dir()?.join(format!("{digest}.zspkg")))
    }

    fn store_blob(&self, bytes: &[u8]) -> Result<(StoredPackage, bool)> {
        if bytes.len() > super::MAX_PACKAGE_BYTES {
            return Err("package exceeds limit".into());
        }
        let digest = sha256(bytes);
        let path = self.blob_path(&digest)?;
        reject_symlink(&path)?;
        if path.exists() {
            let existing = read_bounded(&path, super::MAX_PACKAGE_BYTES)?;
            if existing != bytes {
                return Err("package blob digest collision".into());
            }
            return Ok((StoredPackage::Blob { sha256: digest }, false));
        }
        let directory = self.blob_dir()?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&path)?;
        #[cfg(unix)]
        File::open(path.parent().ok_or("invalid package blob path")?)?.sync_all()?;
        Ok((StoredPackage::Blob { sha256: digest }, true))
    }

    fn cleanup_unreferenced_blobs(&self, state: &State) {
        let referenced: std::collections::BTreeSet<&str> = state
            .packages
            .values()
            .flat_map(|installed| {
                std::iter::once(&installed.current).chain(installed.previous.as_ref())
            })
            .filter_map(StoredPackage::digest)
            .collect();
        let directory = self.root.join("packages");
        let Ok(entries) = fs::read_dir(&directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(digest) = name.strip_suffix(".zspkg") else {
                continue;
            };
            if validate_digest(digest).is_ok() && !referenced.contains(digest) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn advance_generation(state: &mut State, id: &str, previous: u64) -> Result<u64> {
    if !state.generations.contains_key(id) && state.generations.len() >= 4096 {
        return Err("installation history capacity exceeded".into());
    }
    let next = state
        .generations
        .get(id)
        .copied()
        .unwrap_or(0)
        .max(previous)
        .checked_add(1)
        .ok_or("installation generation exhausted")?;
    state.generations.insert(id.into(), next);
    Ok(next)
}
impl StoredPackage {
    fn digest(&self) -> Option<&str> {
        match self {
            Self::Inline(_) => None,
            Self::Blob { sha256 } => Some(sha256),
        }
    }

    fn read(&self, root: &Path) -> Result<Vec<u8>> {
        match self {
            Self::Inline(value) => Ok(value.as_bytes().to_vec()),
            Self::Blob { sha256: digest } => {
                validate_digest(digest)?;
                let directory = root.join("packages");
                reject_symlink(&directory)?;
                let path = directory.join(format!("{digest}.zspkg"));
                reject_symlink(&path)?;
                let bytes = read_bounded(&path, super::MAX_PACKAGE_BYTES)?;
                if sha256(&bytes) != *digest {
                    return Err("installed package blob digest mismatch".into());
                }
                Ok(bytes)
            }
        }
    }
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    reject_regular_if_present(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("package blob exceeds limit".into());
    }
    Ok(bytes)
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid package blob digest".into());
    }
    Ok(())
}
fn reject_regular_if_present(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
            Err("nonregular installation file rejected".into())
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err("symlink installation path rejected".into())
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
