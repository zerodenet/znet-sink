//! Cooperative plugin writer reservation and bounded metadata. Business values
//! never enter the uninstall ledger. A history increment precedes every actual
//! host file writer, including same-value writes; reads do not advance history.
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
thread_local! { static HELD: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) }; }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::services::plugins) struct Epoch {
    pub store_id: String,
    pub epoch: u64,
}
pub(in crate::services::plugins) fn error() -> AppError {
    AppError {
        code: "plugin_uninstall_storage",
        message: "插件卸载材料无法安全核对；原操作保留，不会自动重放".into(),
        details: None,
    }
}
pub(in crate::services::plugins) fn hex(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'a'..=b'f'))
}
pub(in crate::services::plugins) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(in crate::services::plugins) fn random() -> AppResult<String> {
    let mut b = [0; 16];
    getrandom::fill(&mut b).map_err(|_| error())?;
    Ok(b.iter().map(|v| format!("{v:02x}")).collect())
}
fn nofollow(o: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
}
pub(in crate::services::plugins) fn ordinary(path: &Path, directory: bool) -> AppResult<()> {
    let m = fs::symlink_metadata(path).map_err(|_| error())?;
    if m.file_type().is_symlink() || (directory && !m.is_dir()) || (!directory && !m.is_file()) {
        return Err(error());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(error());
        }
    }
    Ok(())
}
pub(in crate::services::plugins) fn read(path: &Path, max: usize) -> AppResult<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(error()),
        Ok(_) => ordinary(path, false)?,
    }
    let mut o = OpenOptions::new();
    o.read(true);
    nofollow(&mut o);
    let f = o.open(path).map_err(|_| error())?;
    if !f.metadata().map_err(|_| error())?.is_file() {
        return Err(error());
    }
    let mut b = Vec::new();
    f.take((max + 1) as u64)
        .read_to_end(&mut b)
        .map_err(|_| error())?;
    if b.len() > max {
        return Err(error());
    }
    Ok(Some(b))
}
pub(in crate::services::plugins) fn write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Ok(m) = fs::symlink_metadata(path) {
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(error());
        }
    }
    let parent = path.parent().ok_or_else(error)?;
    ordinary(parent, true)?;
    crate::services::atomic_file::write(path, bytes).map_err(|_| error())?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| error())?;
    Ok(())
}
pub(in crate::services::plugins) struct RootGuard {
    path: PathBuf,
    file: Option<File>,
}
impl RootGuard {
    pub(in crate::services::plugins) fn acquire(root: &Path) -> AppResult<Self> {
        ordinary(root, true)?;
        let path = fs::canonicalize(root).map_err(|_| error())?;
        if HELD.with(|held| held.borrow().contains(&path)) {
            HELD.with(|h| h.borrow_mut().push(path.clone()));
            return Ok(Self { path, file: None });
        }
        let lock = path.join("operation.lock");
        if fs::symlink_metadata(&lock).is_ok() {
            ordinary(&lock, false)?;
        }
        let mut o = OpenOptions::new();
        o.read(true).write(true).create(true).truncate(false);
        nofollow(&mut o);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let f = o.open(&lock).map_err(|_| error())?;
        if !f.metadata().map_err(|_| error())?.is_file() {
            return Err(error());
        }
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            match f.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < end => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                _ => return Err(error()),
            }
        }
        HELD.with(|h| h.borrow_mut().push(path.clone()));
        Ok(Self {
            path,
            file: Some(f),
        })
    }
    pub(in crate::services::plugins) fn epoch(&self) -> AppResult<Epoch> {
        let path = self.path.join("operation-history.json");
        if let Some(bytes) = read(&path, 4096)? {
            let e: Epoch = serde_json::from_slice(&bytes).map_err(|_| error())?;
            if !hex(&e.store_id, 32) || e.epoch == 0 {
                return Err(error());
            }
            Ok(e)
        } else {
            let e = Epoch {
                store_id: random()?,
                epoch: 1,
            };
            write(&path, &serde_json::to_vec(&e).map_err(|_| error())?)?;
            Ok(e)
        }
    }
    pub(in crate::services::plugins) fn advance(&self) -> AppResult<Epoch> {
        let mut e = self.epoch()?;
        e.epoch = e.epoch.checked_add(1).ok_or_else(error)?;
        write(
            &self.path.join("operation-history.json"),
            &serde_json::to_vec(&e).map_err(|_| error())?,
        )?;
        Ok(e)
    }
}
impl Drop for RootGuard {
    fn drop(&mut self) {
        HELD.with(|h| {
            let mut h = h.borrow_mut();
            if let Some(p) = h.iter().rposition(|p| p == &self.path) {
                h.remove(p);
            }
        });
        if let Some(f) = &self.file {
            let _ = f.unlock();
        }
    }
}
/// Hold through the actual write, even when called without a Host (tests and
/// startup reconstruction). A nested call reuses this thread's original OS lock.
pub(in crate::services::plugins) fn writer(root: &Path) -> AppResult<RootGuard> {
    let g = RootGuard::acquire(root)?;
    g.advance()?;
    Ok(g)
}
/// A plugin's invocation lifetime, shared across cooperating hosts/processes.
/// Uninstall/reinstall only try the exclusive reservation and never wait while
/// holding an operation reservation. Process death releases the kernel lock.
pub(crate) struct Activity(File);
impl Activity {
    pub(in crate::services::plugins) fn acquire(
        root: &Path,
        plugin: &str,
        exclusive: bool,
    ) -> AppResult<Self> {
        ordinary(root, true)?;
        let path = root.join(format!("activity-{}.lock", hash(plugin.as_bytes())));
        if fs::symlink_metadata(&path).is_ok() {
            ordinary(&path, false)?;
        }
        let mut o = OpenOptions::new();
        o.read(true).write(true).create(true).truncate(false);
        nofollow(&mut o);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let f = o.open(path).map_err(|_| error())?;
        if !f.metadata().map_err(|_| error())?.is_file() {
            return Err(error());
        }
        let result = if exclusive {
            f.try_lock()
        } else {
            f.try_lock_shared()
        };
        result.map_err(|_| error())?;
        Ok(Self(f))
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
