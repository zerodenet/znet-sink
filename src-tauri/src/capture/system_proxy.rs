//! System proxy ownership and crash recovery.
//! Markers identify the endpoint set by this client. Disable, quit and stale
//! startup cleanup clear that endpoint instead of restoring an old proxy.
//! The backup is only used to roll back a failed enable/retarget transaction.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::services::data_dir;
use crate::services::system_proxy;

static OPERATION: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ── Marker persistence ──

const MARKER_FILE: &str = "system-proxy-guard.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyMarker {
    host: String,
    port: u16,
    enabled_at_unix_ms: u64,
    /// The user's proxy settings as they were immediately before the GUI
    /// enabled its own proxy. Used only for failed-mutation rollback. Older marker
    /// files written before this field existed deserialize to the default
    /// (proxy off), which is a safe fallback.
    #[serde(default)]
    previous: system_proxy::ProxyBackup,
    #[serde(default)]
    bypass: Vec<String>,
    #[serde(default)]
    bypass_version: u8,
}

fn marker_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(MARKER_FILE))
}

// Resolve the application data directory.

// ── Public API ──

mod cleanup;

/// Clear a stale client proxy on startup; preserve externally changed settings.
pub fn cleanup_on_startup() {
    if let Err(error) = clear_on_exit(None) {
        eprintln!(
            "[ZNet] proxy guard: startup cleanup failed: {}; keeping marker",
            error.message
        );
    }
}

/// Clear the guarded endpoint, or a verified live managed child's local listener.
/// A matching endpoint is required even on quit. No previous proxy is restored.
pub fn clear_on_exit(owned_listener: Option<(String, u16)>) -> AppResult<()> {
    let _operation = OPERATION.lock().unwrap_or_else(|error| error.into_inner());
    cleanup::clear_owned(owned_listener)
}

/// Enable system proxy and write the crash-protection marker.
///
/// Captures the user's current proxy settings into the marker **before**
/// overwriting them, and persists that marker *first* — so a crash at any
/// point (including between enabling the proxy and writing the marker) still
/// leaves a restorable backup on disk.
pub fn enable_with_guard(host: &str, port: u16) -> AppResult<()> {
    enable_with_guard_and_bypass(
        host,
        port,
        &crate::models::app_config::default_proxy_bypass(),
    )
}

pub fn enable_with_guard_and_bypass(host: &str, port: u16, bypass: &[String]) -> AppResult<()> {
    let _operation = OPERATION.lock().unwrap_or_else(|error| error.into_inner());
    let path = marker_path()?;

    // A repeated enable while we still own the current system proxy must be
    // idempotent with respect to the original backup. Re-capturing here would
    // save our own local proxy as `previous`, so a failed mutation's rollback
    // could reinstate the wrong local endpoint.
    if path.exists() {
        match read_marker(&path) {
            Ok(mut marker) => {
                let status = system_proxy::status_fresh()?;
                if status.enabled && status.host == marker.host && status.port == marker.port {
                    let extended = system_proxy::complete_bypass_backup(&mut marker.previous)?;
                    if marker.host == host
                        && marker.port == port
                        && marker.bypass == bypass
                        && marker.bypass_version == 1
                        && !extended
                        && system_proxy::bypass_matches(bypass)?
                    {
                        return Ok(());
                    }

                    write_marker(host, port, marker.previous.clone(), bypass)?;
                    if let Err(error) = system_proxy::enable_with_bypass(host, port, bypass) {
                        if error.code == "authorization_cancelled" {
                            let _ = write_marker(
                                &marker.host,
                                marker.port,
                                marker.previous,
                                &marker.bypass,
                            );
                            return Err(error);
                        }
                        let rollback = system_proxy::enable_with_bypass(
                            &marker.host,
                            marker.port,
                            &marker.bypass,
                        );
                        let _ = write_marker(
                            &marker.host,
                            marker.port,
                            marker.previous,
                            &marker.bypass,
                        );
                        if let Err(rollback_error) = rollback {
                            return Err(AppError::internal(format!(
                                "failed to retarget guarded system proxy: {}; rollback also failed: {}",
                                error.message, rollback_error.message
                            )));
                        }
                        return Err(error);
                    }
                    return Ok(());
                }

                // The marker is stale because the user or another application
                // changed the system proxy after our enable. We no longer own
                // that OS state; discard the old marker and capture the current
                // settings as the new backup before taking ownership again.
                remove_marker_file(&path);
            }
            Err(error) => {
                eprintln!(
                    "[ZNet] proxy guard: existing marker corrupt ({error}), replacing it before enable"
                );
                remove_marker_file(&path);
            }
        }
    }

    let backup = system_proxy::capture_backup()?;
    // Persist the backup BEFORE touching the system. If the app dies right
    // after this line, cleanup_on_startup still knows which endpoint to clear.
    write_marker(host, port, backup.clone(), bypass)?;
    if let Err(error) = system_proxy::enable_with_bypass(host, port, bypass) {
        if error.code == "authorization_cancelled" {
            // A single privileged transaction is not started when the user
            // cancels its authorization dialog, so there is nothing to roll
            // back. Retrying here would immediately show a second dialog.
            remove_marker_file(&path);
            return Err(error);
        }
        eprintln!(
            "[ZNet] proxy guard: enable failed after writing marker, restoring backup: {:?}",
            error
        );
        match system_proxy::restore(&backup) {
            Ok(_) => remove_marker_file(&path),
            Err(restore_error) => {
                // Keep the marker so a later disable/startup cleanup can retry
                // cleanup instead of losing ownership evidence.
                return Err(AppError::internal(format!(
                    "failed to enable system proxy: {}; restoring previous proxy also failed: {}",
                    error.message, restore_error.message
                )));
            }
        }
        return Err(error);
    }
    Ok(())
}

/// Whether the current OS proxy is still the endpoint owned by this GUI.
pub fn is_enabled_by_guard() -> AppResult<bool> {
    let _operation = OPERATION.lock().unwrap_or_else(|error| error.into_inner());
    let path = marker_path()?;
    if !path.exists() {
        return Ok(false);
    }
    let marker = read_marker(&path).map_err(|error| {
        crate::errors::AppError::internal(format!("failed to read proxy marker: {error}"))
    })?;
    let status = system_proxy::status()?;
    Ok(owns_endpoint(
        status.enabled,
        &status.host,
        status.port,
        &marker.host,
        marker.port,
    ))
}

/// Move a GUI-owned system proxy to a new local endpoint without replacing
/// the original user-proxy backup stored in the crash marker.
pub fn retarget_if_enabled(host: &str, port: u16) -> AppResult<()> {
    let _operation = OPERATION.lock().unwrap_or_else(|error| error.into_inner());
    let path = marker_path()?;
    if !path.exists() {
        return Ok(());
    }
    let marker = read_marker(&path).map_err(|error| {
        crate::errors::AppError::internal(format!("failed to read proxy marker: {error}"))
    })?;
    let status = system_proxy::status_fresh()?;
    if !status.enabled || status.host != marker.host || status.port != marker.port {
        return Ok(());
    }
    if marker.host == host && marker.port == port {
        return Ok(());
    }

    write_marker(host, port, marker.previous.clone(), &marker.bypass)?;
    if let Err(error) = system_proxy::enable_with_bypass(host, port, &marker.bypass) {
        if error.code == "authorization_cancelled" {
            let _ = write_marker(&marker.host, marker.port, marker.previous, &marker.bypass);
            return Err(error);
        }
        let rollback = system_proxy::enable_with_bypass(&marker.host, marker.port, &marker.bypass);
        let _ = write_marker(&marker.host, marker.port, marker.previous, &marker.bypass);
        if let Err(rollback_error) = rollback {
            return Err(AppError::internal(format!(
                "failed to retarget guarded system proxy: {}; rollback also failed: {}",
                error.message, rollback_error.message
            )));
        }
        return Err(error);
    }
    Ok(())
}

/// Clear the currently guarded proxy. No marker means no ownership authority.
pub fn disable_with_guard() -> AppResult<()> {
    clear_on_exit(None)
}

/// Best-effort cleanup on Rust panics. Forced kills are handled next startup.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        #[cfg(target_os = "macos")]
        eprintln!("[ZNet] panic guard: preserving proxy marker for startup cleanup; skipping interactive macOS cleanup");
        #[cfg(not(target_os = "macos"))]
        let _ = disable_after_panic();
        (original)(info);
    }));
}

// ── Marker I/O ──

fn read_marker(path: &PathBuf) -> Result<ProxyMarker, String> {
    fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<ProxyMarker>(&s).map_err(|e| e.to_string()))
}

fn write_marker(
    host: &str,
    port: u16,
    previous: system_proxy::ProxyBackup,
    bypass: &[String],
) -> AppResult<()> {
    let path = marker_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            crate::errors::AppError::internal(format!("failed to create marker dir: {e}"))
        })?;
    }
    let marker = ProxyMarker {
        host: host.to_string(),
        port,
        enabled_at_unix_ms: crate::services::common::now_unix_ms(),
        previous,
        bypass: bypass.to_vec(),
        bypass_version: 1,
    };
    let json = serde_json::to_string_pretty(&marker).map_err(|e| {
        crate::errors::AppError::internal(format!("failed to serialize proxy marker: {e}"))
    })?;
    fs::write(&path, json).map_err(|e| {
        crate::errors::AppError::internal(format!("failed to write proxy marker: {e}"))
    })?;
    eprintln!(
        "[ZNet] proxy guard: marker written ({}:{}; backup enabled={})",
        host, port, marker.previous.enabled
    );
    Ok(())
}

fn remove_marker_file(path: &PathBuf) {
    if path.exists() {
        if let Err(e) = fs::remove_file(path) {
            eprintln!("[ZNet] proxy guard: failed to remove marker: {e}");
        } else {
            eprintln!("[ZNet] proxy guard: marker removed");
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn disable_after_panic() -> AppResult<()> {
    let Ok(_operation) = OPERATION.try_lock() else {
        return Ok(());
    };
    cleanup::clear_owned(None)
}

fn owns_endpoint(enabled: bool, host: &str, port: u16, owned_host: &str, owned_port: u16) -> bool {
    enabled && host == owned_host && port == owned_port
}
#[cfg(test)]
#[path = "system_proxy_tests.rs"]
mod tests;
