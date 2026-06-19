//! Crash-safe system proxy lifecycle management.
//!
//! Strategy:
//!   1. **Backup + Marker file** — When the GUI enables the system proxy it
//!      first snapshots the user's original proxy settings into the marker
//!      file. The marker survives all crash types (panic, SIGKILL, power
//!      loss, Task Manager kill).
//!   2. **Restore-on-disable** — A disable is only ever performed when a
//!      marker exists (i.e. the GUI itself enabled the proxy). Instead of
//!      blanking the proxy it *restores* the captured backup, so the user's
//!      pre-existing proxy (e.g. their own `127.0.0.1:1080`) is recovered.
//!      If the GUI never enabled the proxy, disable is a no-op — the user's
//!      settings are left completely untouched.
//!   3. **Startup cleanup** — On every launch, check for a stale marker. If
//!      found and the proxy still points at our endpoint, restore the backup.
//!   4. **Panic hook** — Best-effort restore on Rust panics.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::data_dir;
use crate::errors::AppResult;
use crate::services::system_proxy;

// ── Marker persistence ──

const MARKER_FILE: &str = "system-proxy-guard.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyMarker {
    host: String,
    port: u16,
    enabled_at_unix_ms: u64,
    /// The user's proxy settings as they were immediately before the GUI
    /// enabled its own proxy. Restored verbatim on disable. Older marker
    /// files written before this field existed deserialize to the default
    /// (proxy off), which is a safe fallback.
    #[serde(default)]
    previous: system_proxy::ProxyBackup,
}

fn marker_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(MARKER_FILE))
}

/// Resolve the application data directory.

// ── Public API ──

/// Call on every app startup — detects and cleans up a stale system proxy
/// left behind after a crash / forced kill.
///
/// Only acts when a marker exists *and* the current proxy still points at
/// the endpoint the GUI set. If the user has since changed the proxy
/// themselves, their settings are left alone. Restores the captured backup
/// rather than blanking.
pub fn cleanup_on_startup() {
    let path = match marker_path() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[ZNet] proxy guard: cannot resolve marker path: {:?}", e);
            return;
        }
    };

    if !path.exists() {
        return;
    }

    eprintln!("[ZNet] proxy guard: stale marker detected, attempting cleanup");

    // Read marker to verify ownership
    let marker: ProxyMarker = match read_marker(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[ZNet] proxy guard: marker file corrupt ({e}), removing");
            let _ = fs::remove_file(&path);
            return;
        }
    };

    // Only restore if the current system proxy still matches our endpoint —
    // otherwise the user changed it after the crash and we must not touch it.
    match system_proxy::status() {
        Ok(status)
            if status.enabled && status.host == marker.host && status.port == marker.port =>
        {
            eprintln!(
                "[ZNet] proxy guard: restoring user proxy after stale GUI proxy ({}:{})",
                marker.host, marker.port
            );
            if let Err(e) = system_proxy::restore(&marker.previous) {
                eprintln!(
                    "[ZNet] proxy guard: failed to restore previous proxy: {:?}, falling back to disable",
                    e
                );
                let _ = system_proxy::disable();
            }
        }
        Ok(status) => {
            eprintln!(
                "[ZNet] proxy guard: system proxy changed since crash (current: {}:{}, ours: {}:{}), not touching it",
                status.host, status.port, marker.host, marker.port
            );
        }
        Err(e) => {
            eprintln!("[ZNet] proxy guard: cannot read proxy status: {:?}", e);
        }
    }

    // Always remove the marker
    remove_marker_file(&path);

    eprintln!("[ZNet] proxy guard: startup cleanup complete");
}

/// Enable system proxy and write the crash-protection marker.
///
/// Captures the user's current proxy settings into the marker **before**
/// overwriting them, and persists that marker *first* — so a crash at any
/// point (including between enabling the proxy and writing the marker) still
/// leaves a restorable backup on disk.
pub fn enable_with_guard(host: &str, port: u16) -> AppResult<()> {
    let backup = system_proxy::capture_backup()?;
    // Persist the backup BEFORE touching the system. If the app dies right
    // after this line, cleanup_on_startup still has the backup to restore.
    write_marker(host, port, backup)?;
    if let Err(error) = system_proxy::enable(host, port) {
        // Enable failed — the proxy was never actually changed, so the
        // marker we just wrote would be misleading. Roll it back so a
        // later disable doesn't try to restore a state that never
        // existed (harmless, but keeps the bookkeeping honest).
        eprintln!(
            "[ZNet] proxy guard: enable failed after writing marker, rolling marker back: {:?}",
            error
        );
        if let Ok(path) = marker_path() {
            remove_marker_file(&path);
        }
        return Err(error);
    }
    Ok(())
}

/// Restore the user's system proxy and remove the crash-protection marker.
///
/// This is a **no-op when no marker exists** — i.e. when the GUI never
/// enabled the proxy. That is what prevents kernel stop / app exit / crash
/// recovery from destroying a proxy the user configured independently of
/// the GUI. When a marker exists, the captured backup is restored instead
/// of the proxy being blanked.
pub fn disable_with_guard() -> AppResult<()> {
    let path = marker_path()?;
    if !path.exists() {
        // The GUI never enabled the proxy — leave the user's settings alone.
        return Ok(());
    }

    let marker = match read_marker(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[ZNet] proxy guard: marker corrupt ({e}), removing without restoring");
            remove_marker_file(&path);
            return Ok(());
        }
    };

    eprintln!(
        "[ZNet] proxy guard: restoring user proxy (was set to {}:{})",
        marker.host, marker.port
    );
    if let Err(e) = system_proxy::restore(&marker.previous) {
        eprintln!(
            "[ZNet] proxy guard: failed to restore previous proxy: {:?}, falling back to disable",
            e
        );
        let _ = system_proxy::disable();
    }
    remove_marker_file(&path);
    Ok(())
}

/// Install a panic hook that attempts to restore system proxy on Rust panics.
/// This is a best-effort measure — the marker file handles the SIGKILL case.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("[ZNet] panic guard: attempting emergency proxy restore");
        // Only restore if we own a marker; never touch a proxy the GUI
        // didn't set.
        if let Ok(path) = marker_path() {
            if let Ok(marker) = read_marker(&path) {
                let _ = system_proxy::restore(&marker.previous);
                let _ = fs::remove_file(&path);
            }
        }
        (original)(info);
    }));
}

// ── Marker I/O ──

fn read_marker(path: &PathBuf) -> Result<ProxyMarker, String> {
    fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<ProxyMarker>(&s).map_err(|e| e.to_string()))
}

fn write_marker(host: &str, port: u16, previous: system_proxy::ProxyBackup) -> AppResult<()> {
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
