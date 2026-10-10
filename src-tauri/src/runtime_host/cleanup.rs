use crate::errors::{AppError, AppResult};

/// Independent capture layers get one attempt each. Never abandon later
/// cleanup because an earlier layer failed, and preserve all reported errors.
pub(super) fn release_capture(
    cleanup_proxy: bool,
    release_dns: impl FnOnce() -> AppResult<()>,
    release_proxy: impl FnOnce() -> AppResult<()>,
) -> AppResult<()> {
    let mut errors = Vec::new();
    if let Err(error) = release_dns() {
        errors.push(format!("system DNS restore failed: {}", error.message));
    }
    if cleanup_proxy {
        if let Err(error) = release_proxy() {
            errors.push(format!("system proxy cleanup failed: {}", error.message));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::internal(errors.join("; ")))
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
