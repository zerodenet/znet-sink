//! Client-side node latency probing.
//!
//! Orchestrates speed tests (queue, concurrency, progress) on the client side.
//! Individual node probes go directly to the core engine's IPC without any
//! upfront health check — each probe handles its own timeout and failure.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::errors::AppError;
use crate::kernel::zero::commands;
use crate::services::{common, core_config};
use crate::state::app_state::AppState;

/// Maximum concurrent probe requests to the core.
pub const MAX_CONCURRENT_PROBES: usize = 8;

/// Per-node probe result.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub target_tag: String,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

/// Probe a single node via the core's probe command.
/// No upfront health check — the probe itself handles timeout/failure.
pub async fn probe_single(state: &AppState, target_tag: &str) -> ProbeResult {
    let target_tag = target_tag.trim().to_string();
    if target_tag.is_empty() {
        return ProbeResult {
            target_tag: target_tag.clone(),
            reachable: false,
            latency_ms: None,
            message: Some("target tag must not be empty".to_string()),
        };
    }

    // Build IPC options from app config
    let options = match default_ipc_options(state) {
        Ok(opts) => opts,
        Err(e) => {
            return ProbeResult {
                target_tag,
                reachable: false,
                latency_ms: None,
                message: Some(format!("IPC config error: {}", e.message)),
            };
        }
    };

    // Send probe command directly — no readiness health check
    match commands::probe_target(target_tag.clone(), options).await {
        Ok(result) => ProbeResult {
            target_tag: result.target_tag,
            reachable: result.reachable,
            latency_ms: result.latency_ms,
            message: result.message,
        },
        Err(e) => ProbeResult {
            target_tag,
            reachable: false,
            latency_ms: None,
            message: Some(e.message),
        },
    }
}

/// Run a batch probe with bounded concurrency. Emits Tauri events for each result.
///
/// Events:
/// - `probe:progress` — `{ done, total }`
/// - `probe:result`   — `ProbeResult`
/// - `probe:complete` — `{ total, reachable, failed }`
pub async fn run_probe_batch(
    app_handle: AppHandle,
    target_tags: Vec<String>,
    max_concurrent: usize,
) {
    let total = target_tags.len();
    if total == 0 {
        return;
    }

    let max_concurrent = max_concurrent.clamp(1, MAX_CONCURRENT_PROBES);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    let done_count = Arc::new(AtomicUsize::new(0));
    let reachable_count = Arc::new(AtomicUsize::new(0));
    let failed_count = Arc::new(AtomicUsize::new(0));

    // Emit initial progress
    let _ = app_handle.emit(
        "probe:progress",
        serde_json::json!({ "done": 0, "total": total }),
    );

    let mut handles = Vec::with_capacity(total);

    for target_tag in target_tags {
        let permit = semaphore.clone().acquire_owned().await.ok();
        let app_handle = app_handle.clone();
        let done_count = done_count.clone();
        let reachable_count = reachable_count.clone();
        let failed_count = failed_count.clone();

        let handle = tauri::async_runtime::spawn(async move {
            let state = app_handle.state::<AppState>();
            let result = probe_single(state.inner(), &target_tag).await;

            if result.reachable {
                reachable_count.fetch_add(1, Ordering::Relaxed);
            } else {
                failed_count.fetch_add(1, Ordering::Relaxed);
            }
            let done = done_count.fetch_add(1, Ordering::Relaxed) + 1;

            let _ = app_handle.emit("probe:result", &result);
            let _ = app_handle.emit(
                "probe:progress",
                serde_json::json!({ "done": done, "total": total }),
            );

            drop(permit);
            result
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    let _ = app_handle.emit(
        "probe:complete",
        serde_json::json!({
            "total": total,
            "reachable": reachable_count.load(Ordering::Relaxed),
            "failed": failed_count.load(Ordering::Relaxed),
        }),
    );
}

fn default_ipc_options(
    state: &AppState,
) -> Result<Option<crate::models::core::CoreIpcOptions>, AppError> {
    let config = common::lock(state.app_config(), "app_config")?.core.clone();
    Ok(Some(core_config::ipc_options_from_app_config(&config)))
}
