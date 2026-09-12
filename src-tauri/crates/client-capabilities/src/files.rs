//! Host-created file selections, never raw paths supplied by a guest invocation.
use std::{
    fs::OpenOptions,
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use znet_client_core::capability::{Error, Lease, Permission, Resource};
static NEXT_SELECTION: AtomicU64 = AtomicU64::new(1);
pub struct Selection {
    path: PathBuf,
    id: u64,
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
