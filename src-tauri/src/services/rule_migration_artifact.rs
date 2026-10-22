//! Read-only proof of one already-authorized GUI-owned artifact. Resource
//! ownership/CAS belongs to the migration owner, not to a matching path.
use crate::{
    errors::{AppError, AppResult},
    models::rule_set::RuleSetProfile,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
fn unavailable() -> AppError {
    AppError {
        code: "rule_migration_artifact_unavailable",
        message: "替代规则集的宿主产物缺失、损坏或声明不匹配；未迁移或删除，请先重新生成有效产物"
            .into(),
        details: None,
    }
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 512
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[cfg(unix)]
fn open_owned(dir: &Path, id: &str, name: &str) -> AppResult<File> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::OpenOptionsExt,
        },
    };
    let root = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(dir)
        .map_err(|_| unavailable())?;
    fn child(parent: &File, name: &str, directory: bool) -> AppResult<File> {
        let name = CString::new(name).map_err(|_| unavailable())?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // All paths are single validated components. Descriptor-relative opens
        // cannot follow a replaced parent path into another owner directory.
        let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(unavailable());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    let artifacts = child(&root, "rule-artifacts", true)?;
    let owner = child(&artifacts, id, true)?;
    child(&owner, name, false)
}
#[cfg(not(unix))]
fn open_owned(dir: &Path, id: &str, name: &str) -> AppResult<File> {
    fn directory(path: &Path) -> AppResult<()> {
        let m = std::fs::symlink_metadata(path).map_err(|_| unavailable())?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(unavailable());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if m.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(unavailable());
            }
        }
        Ok(())
    }
    directory(dir)?;
    let artifacts = dir.join("rule-artifacts");
    directory(&artifacts)?;
    let owner = artifacts.join(id);
    directory(&owner)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(owner.join(name)).map_err(|_| unavailable())?;
    directory(&owner)?;
    directory(&artifacts)?;
    Ok(file)
}
pub(crate) fn verify(dir: &Path, profile: &RuleSetProfile) -> AppResult<String> {
    let artifact = profile.artifact.as_ref().ok_or_else(unavailable)?;
    if !dir.is_absolute()
        || !safe_id(&profile.id)
        || artifact.file_size == 0
        || artifact.file_size > MAX_BYTES
    {
        return Err(unavailable());
    }
    let filename = format!(
        "{}-{:08x}.zrs",
        artifact.built_at_unix_ms, artifact.checksum
    );
    if Path::new(&artifact.path) != dir.join("rule-artifacts").join(&profile.id).join(&filename) {
        return Err(unavailable());
    }
    let file = open_owned(dir, &profile.id, &filename)?;
    let m = file.metadata().map_err(|_| unavailable())?;
    if !m.is_file() || m.len() != artifact.file_size || m.len() > MAX_BYTES {
        return Err(unavailable());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(unavailable());
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 != artifact.file_size || bytes.len() as u64 > MAX_BYTES {
        return Err(unavailable());
    }
    let metadata = zero_rule::zrs::verify(&bytes, zero_rule::zrs::VerifyMode::FullChecksum)
        .map_err(|_| unavailable())?;
    if metadata.file_size != artifact.file_size
        || metadata.body_checksum != artifact.checksum
        || metadata.major_version != artifact.major_version
        || metadata.minor_version != artifact.minor_version
        || metadata.entry_count() != artifact.entry_count
    {
        return Err(unavailable());
    }
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

#[cfg(test)]
mod tests;
