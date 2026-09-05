use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Metadata {
    pub etag: Option<String>,
    pub total: Option<u64>,
    pub complete: bool,
}

pub(super) struct Cache {
    pub part: PathBuf,
    pub meta: Metadata,
    root: PathBuf,
    _lock: File,
}

impl Cache {
    pub fn open(parent: &Path, url: &str, identity: &str) -> AppResult<Self> {
        fs::create_dir_all(parent).map_err(io_error)?;
        prune(parent);
        let key = format!("{:x}", Sha256::digest(format!("{url}\0{identity}")));
        let root = parent.join(key);
        fs::create_dir_all(&root).map_err(io_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("lock"))
            .map_err(io_error)?;
        lock.try_lock()
            .map_err(|_| AppError::internal("该文件正在下载或校验，请等待当前操作完成"))?;
        let meta = fs::read(root.join("metadata.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let cache = Self {
            part: root.join("payload.part"),
            meta,
            root,
            _lock: lock,
        };
        cache.save()?;
        Ok(cache)
    }
    pub fn len(&self) -> AppResult<u64> {
        match fs::metadata(&self.part) {
            Ok(m) => Ok(m.len()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(io_error(e)),
        }
    }
    pub fn reset(&self) -> AppResult<()> {
        for path in [&self.part, &self.root.join("metadata.json")] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_error(e)),
            }
        }
        Ok(())
    }
    pub fn save(&self) -> AppResult<()> {
        use std::io::Write;
        let mut temp = tempfile::NamedTempFile::new_in(&self.root).map_err(io_error)?;
        temp.write_all(
            &serde_json::to_vec(&self.meta).map_err(|e| AppError::internal(e.to_string()))?,
        )
        .map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        temp.persist(self.root.join("metadata.json"))
            .map_err(|e| io_error(e.error))?;
        Ok(())
    }
}

// Only expired cache entries owned by this downloader are eligible. Active OS
// locks are respected; crashing the application releases locks automatically.
fn prune(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let root = entry.path();
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let expired = fs::metadata(root.join("metadata.json"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > Duration::from_secs(7 * 86400));
        if !expired {
            continue;
        }
        let Ok(lock) = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("lock"))
        else {
            continue;
        };
        if lock.try_lock().is_ok() {
            // Keep the lock inode/directory: deleting it could admit a competing
            // downloader while this lock is still held (especially on Unix).
            let _ = fs::remove_file(root.join("payload.part"));
            let _ = fs::remove_file(root.join("metadata.json"));
        }
    }
}

pub(super) fn io_error(error: std::io::Error) -> AppError {
    AppError::internal(format!("下载缓存读写失败（请检查磁盘空间和权限）：{error}"))
}
