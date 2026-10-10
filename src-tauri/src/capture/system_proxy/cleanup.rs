use super::{marker_path, read_marker, remove_marker_file};
use crate::{errors::AppResult, services::system_proxy};

pub(super) fn clear_owned(owned_listener: Option<(String, u16)>) -> AppResult<()> {
    clear_at(marker_path()?, owned_listener, system_proxy::clear_matching)
}

fn clear_at(
    path: std::path::PathBuf,
    owned_listener: Option<(String, u16)>,
    clear: impl FnOnce(&[(String, u16)]) -> AppResult<()>,
) -> AppResult<()> {
    let marker = if path.exists() {
        Some(read_marker(&path).map_err(crate::errors::AppError::internal)?)
    } else {
        None
    };
    let mut endpoints = Vec::new();
    if let Some(marker) = marker {
        endpoints.push((marker.host, marker.port));
    }
    if let Some(endpoint) = owned_listener {
        endpoints.push(endpoint);
    }
    if endpoints.is_empty() {
        return Ok(());
    }
    clear(&endpoints)?;
    // On failure the marker remains for recovery. If externally changed,
    // retire our historical marker without touching the current proxy.
    remove_marker_file(&path);
    Ok(())
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
