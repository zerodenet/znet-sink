//! Publish complete files without truncating the previous generation.

use std::io::{self, Write};
use std::path::Path;

pub(crate) fn write(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(content)?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    // The replacement has committed. A directory sync failure must not tell
    // callers that the old generation is still on disk.
    #[cfg(unix)]
    if let Ok(directory) = std::fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

/// Publish only while the original authority still permits the write. The
/// final check follows staging, fsync, and every preceding domain/file wait.
pub(crate) fn write_authorized(
    path: &Path,
    content: &[u8],
    authorize: &dyn Fn() -> crate::errors::AppResult<()>,
) -> crate::errors::AppResult<()> {
    let failure = || crate::errors::AppError::internal("local file publication failed");
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|_| failure())?;
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|_| failure())?;
    staged.write_all(content).map_err(|_| failure())?;
    staged.as_file().sync_all().map_err(|_| failure())?;
    authorize()?;
    staged.persist(path).map_err(|_| failure())?;
    #[cfg(unix)]
    if let Ok(directory) = std::fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

pub(crate) fn copy(source: &Path, target: &Path) -> io::Result<()> {
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    let mut input = std::fs::File::open(source)?;
    std::io::copy(&mut input, &mut staged)?;
    staged
        .as_file()
        .set_permissions(input.metadata()?.permissions())?;
    staged.as_file().sync_all()?;
    staged.persist(target).map_err(|error| error.error)?;
    #[cfg(unix)]
    if let Ok(directory) = std::fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}
