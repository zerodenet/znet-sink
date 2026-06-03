//! Crash-safe system proxy lifecycle management.
//!
//! Strategy:
//!   1. **Marker file** — Written when system proxy is enabled, removed on clean disable.
//!      Survives all crash types (panic, SIGKILL, power loss, Task Manager kill).
//!   2. **Startup cleanup** — On every launch, check for a stale marker. If found,
//!      verify the proxy still points to our endpoint and disable it.
//!   3. **Panic hook** — Best-effort cleanup on Rust panics (catches most graceful crashes).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::services::system_proxy;

// ── Marker persistence ──

const MARKER_FILE: &str = "system-proxy-guard.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyMarker {
    host: String,
    port: u16,
    enabled_at_unix_ms: u64,
}

fn marker_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(MARKER_FILE))
}

/// Resolve the application data directory.

// ── Public API ──

/// Call on every app startup — detects and cleans up a stale system proxy
/// left behind after a crash / forced kill.
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
    let marker: ProxyMarker = match fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str::<ProxyMarker>(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[ZNet] proxy guard: marker file corrupt ({e}), removing");
            let _ = fs::remove_file(&path);
            return;
        }
    };

    // Only disable if the current system proxy still matches our endpoint
    match system_proxy::status() {
        Ok(status) if status.enabled && status.host == marker.host && status.port == marker.port => {
            eprintln!(
                "[ZNet] proxy guard: disabling stale system proxy ({}:{})",
                marker.host, marker.port
            );
            if let Err(e) = system_proxy::disable() {
                eprintln!("[ZNet] proxy guard: failed to disable stale proxy: {:?}", e);
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
    if let Err(e) = fs::remove_file(&path) {
        eprintln!("[ZNet] proxy guard: failed to remove stale marker: {e}");
    }

    eprintln!("[ZNet] proxy guard: startup cleanup complete");
}

/// Enable system proxy and write the crash-protection marker.
pub fn enable_with_guard(host: &str, port: u16) -> AppResult<()> {
    system_proxy::enable(host, port)?;
    write_marker(host, port)?;
    Ok(())
}

/// Disable system proxy and remove the crash-protection marker.
pub fn disable_with_guard() -> AppResult<()> {
    let _ = system_proxy::disable(); // best-effort
    remove_marker();
    Ok(())
}

/// Install a panic hook that attempts to disable system proxy on Rust panics.
/// This is a best-effort measure — the marker file handles the SIGKILL case.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("[ZNet] panic guard: attempting emergency proxy cleanup");
        let _ = system_proxy::disable();
        if let Ok(path) = marker_path() {
            let _ = fs::remove_file(path);
        }
        (original)(info);
    }));
}

// ── Marker I/O ──

fn write_marker(host: &str, port: u16) -> AppResult<()> {
    let path = marker_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::internal(format!("failed to create marker dir: {e}")))?;
    }
    let marker = ProxyMarker {
        host: host.to_string(),
        port,
        enabled_at_unix_ms: crate::services::common::now_unix_ms(),
    };
    let json = serde_json::to_string_pretty(&marker)
        .map_err(|e| AppError::internal(format!("failed to serialize proxy marker: {e}")))?;
    fs::write(&path, json)
        .map_err(|e| AppError::internal(format!("failed to write proxy marker: {e}")))?;
    eprintln!("[ZNet] proxy guard: marker written ({}:{})", host, port);
    Ok(())
}

fn remove_marker() {
    match marker_path() {
        Ok(path) if path.exists() => {
            if let Err(e) = fs::remove_file(&path) {
                eprintln!("[ZNet] proxy guard: failed to remove marker: {e}");
            } else {
                eprintln!("[ZNet] proxy guard: marker removed");
            }
        }
        Ok(_) => {} // marker already gone
        Err(e) => eprintln!("[ZNet] proxy guard: cannot resolve marker path: {:?}", e),
    }
}
