//! Host-created file selections, never raw paths supplied by a guest invocation.
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use znet_client_core::capability::{Error, Lease, Permission, Resource};
static NEXT_SELECTION: AtomicU64 = AtomicU64::new(1);
pub struct Selection {
    path: PathBuf,
    id: u64,
}

pub struct Publication {
    path: PathBuf,
    id: u64,
}
impl Publication {
    /// Native storage owner only. The path never crosses a guest boundary.
    pub fn from_host_path(path: PathBuf) -> Self {
        Self {
            path,
            id: NEXT_SELECTION.fetch_add(1, Ordering::Relaxed),
        }
    }
    pub fn permission(&self) -> Permission {
        Permission::new("file.publish", format!("publication:{}", self.id))
    }
}

/// Publish exact bytes atomically. The temporary file is read back before the
/// rename so a partial or altered write never becomes the visible generation.
pub fn publish(
    lease: &Lease,
    target: &Publication,
    bytes: &[u8],
    max_bytes: usize,
) -> Result<Resource<PathBuf>, Error> {
    lease.execute(&target.permission(), || {
        if bytes.is_empty()
            || bytes.len() > max_bytes
            || max_bytes == 0
            || max_bytes > 64 * 1024 * 1024
        {
            return Err(Error::BudgetExceeded);
        }
        let parent = target.path.parent().ok_or(Error::InvalidRequest)?;
        std::fs::create_dir_all(parent).map_err(|_| Error::Transport)?;
        let temporary = parent.join(format!(
            ".{}.{}.tmp",
            target
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(Error::InvalidRequest)?,
            target.id
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| Error::Transport)?;
            let mut offset = 0;
            for chunk in bytes.chunks(64 * 1024) {
                lease.check(None)?;
                file.write_all(chunk).map_err(|_| Error::Transport)?;
                offset += chunk.len();
            }
            file.sync_all().map_err(|_| Error::Transport)?;
            drop(file);
            let written = std::fs::read(&temporary).map_err(|_| Error::Transport)?;
            if written != bytes || offset != bytes.len() {
                return Err(Error::Transport);
            }
            let backup = parent.join(format!(
                ".{}.{}.backup",
                target
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(Error::InvalidRequest)?,
                target.id
            ));
            let had_previous = match std::fs::symlink_metadata(&target.path) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    std::fs::rename(&target.path, &backup).map_err(|_| Error::Transport)?;
                    true
                }
                Ok(_) => return Err(Error::InvalidRequest),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => return Err(Error::Transport),
            };
            if std::fs::rename(&temporary, &target.path).is_err() {
                if had_previous {
                    let _ = std::fs::rename(&backup, &target.path);
                }
                return Err(Error::Transport);
            }
            if had_previous {
                let _ = std::fs::remove_file(&backup);
            }
            if let Ok(directory) = std::fs::File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok::<_, Error>((target.path.clone(), bytes.len()))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    })
}
impl Selection {
    /// Native UI/file-picker adapter only. Pure selection, no IO before authorization.
    pub fn from_user_path(path: PathBuf) -> Self {
        Self {
            path,
            id: NEXT_SELECTION.fetch_add(1, Ordering::Relaxed),
        }
    }
    pub fn permission(&self) -> Permission {
        Permission::new("file.read", format!("selection:{}", self.id))
    }
}
pub fn read(
    lease: &Lease,
    selected: &Selection,
    max_bytes: usize,
) -> Result<Resource<Vec<u8>>, Error> {
    lease.execute(&selected.permission(), || {
        if max_bytes == 0 || max_bytes > 64 * 1024 * 1024 {
            return Err(Error::BudgetExceeded);
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let mut file = options.open(&selected.path).map_err(|_| Error::Transport)?;
        let metadata = file.metadata().map_err(|_| Error::Transport)?;
        if !metadata.is_file() {
            return Err(Error::InvalidRequest);
        }
        if metadata.len() > max_bytes as u64 {
            return Err(Error::BudgetExceeded);
        }
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            lease.check(None)?;
            let count = file.read(&mut buffer).map_err(|_| Error::Transport)?;
            if count == 0 {
                break;
            }
            if count > max_bytes.saturating_sub(bytes.len()) {
                return Err(Error::BudgetExceeded);
            }
            bytes.extend_from_slice(&buffer[..count]);
        }
        let size = bytes.len();
        Ok((bytes, size))
    })
}
