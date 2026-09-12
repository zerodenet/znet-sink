use super::{
    directory::Registration,
    package::{verify, VerifiedPackage},
    Result,
};
use crate::contract::Target;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_STORE: usize = 16 * 1024 * 1024;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    packages: BTreeMap<String, Installed>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    current: String,
    previous: Option<String>,
}
/// One bounded atomic state file. No untrusted archive paths or executable extraction.
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
    fn lock(&self) -> Result<File> {
        let path = self.root.join("install.lock");
        reject_symlink(&path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(file)
    }
    fn read(&self) -> Result<State> {
        let path = self.root.join("installed.json");
        reject_symlink(&path)?;
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
        if state.packages.len() > 16 {
            return Err("too many installed packages".into());
        }
        Ok(state)
    }
    fn save(&self, state: &State) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        if bytes.len() > MAX_STORE || state.packages.len() > 16 {
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
            let old_verified = verify(old.current.as_bytes(), registration)?;
            if semver::Version::parse(&verified.version)?
                <= semver::Version::parse(&old_verified.version)?
            {
                return Err("install requires a newer version; use explicit rollback".into());
            }
            Some(old.current.clone())
        } else {
            None
        };
        state.packages.insert(
            registration.id.clone(),
            Installed {
                current: String::from_utf8(bytes.to_vec())?,
                previous,
            },
        );
        self.save(&state)?;
        Ok(verified)
    }
    pub fn current(&self, registration: &Registration) -> Result<VerifiedPackage> {
        let _lock = self.lock()?;
        let state = self.read()?;
        let entry = state
            .packages
            .get(&registration.id)
            .ok_or("plugin is not installed")?;
        verify(entry.current.as_bytes(), registration)
    }
    pub fn list(&self) -> Result<Vec<String>> {
        let _lock = self.lock()?;
        Ok(self.read()?.packages.into_keys().collect())
    }
    pub fn uninstall(&self, id: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut state = self.read()?;
        if state.packages.remove(id).is_none() {
            return Err("plugin is not installed".into());
        }
        self.save(&state)
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
        let verified = verify(previous.as_bytes(), registration)?;
        verified.compatible(target, host_version)?;
        let previous = entry.previous.take().unwrap();
        entry.previous = Some(std::mem::replace(&mut entry.current, previous));
        self.save(&state)?;
        Ok(verified)
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
