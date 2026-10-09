use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

mod capture;
pub use capture::CaptureStatus;
pub(crate) use capture::{capture_payload, capture_status, set_capture};

mod worker;

/// A captured IPC frame for the debug diagnostic page.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFrame {
    /// Monotonic sequence number.
    pub id: u64,
    /// Unix ms timestamp when captured.
    pub at_ms: u64,
    /// "tx" (GUI → kernel) or "rx" (kernel → GUI).
    #[serde(rename = "direction")]
    pub direction: String,
    /// Transport classification such as query, subscribe-ack, response, or event.
    pub frame_type: String,
    /// The JSON payload (may be truncated for large responses).
    pub payload: serde_json::Value,
    /// Elapsed ms since the matching request (response frames only).
    pub elapsed_ms: Option<u64>,
    /// Error string if the request failed.
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFrameQuery {
    pub frame_type: Option<String>,
    pub limit: Option<usize>,
    pub before_id: Option<u64>,
    /// Connection-history-only text search across the persisted event record.
    pub search: Option<String>,
    /// Connection-history-only exact protocol/network filter.
    pub protocol: Option<String>,
    /// Connection-history-only exact outbound tag filter.
    pub outbound: Option<String>,
    /// Connection-history-only exact outcome or close-reason filter.
    pub outcome: Option<String>,
    /// Inclusive client capture-time bounds for connection history.
    pub captured_after_ms: Option<u64>,
    pub captured_before_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugFramePage {
    pub items: Vec<DebugFrame>,
    pub has_more: bool,
    pub oldest_available_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<crate::services::connection_history_store::HistorySummary>,
}

static DEBUG_FRAME_ID: AtomicU64 = AtomicU64::new(0);

type DebugFrameObserver = Arc<dyn Fn(&DebugFrame) + Send + Sync + 'static>;
static DEBUG_FRAME_OBSERVER: OnceLock<DebugFrameObserver> = OnceLock::new();
static DEBUG_WORKER: OnceLock<Option<worker::Worker<DebugFrame>>> = OnceLock::new();

/// Install one process-wide observer for captured IPC frames. The transport
/// remains independent of application state; the Tauri composition root
/// decides whether and how frames are projected into user-visible logs.
pub(crate) fn install_debug_frame_observer(observer: DebugFrameObserver) -> bool {
    DEBUG_FRAME_OBSERVER.set(observer).is_ok()
}

/// Transport producers project borrowed JSON before cloning any diagnostic data.
pub(crate) fn push_debug_frame(mut frame: DebugFrame) {
    // Small transport-generated records use the same bounds/redaction. Large
    // producer payloads were already projected from a borrow before this call.
    if frame.payload.get("_diagnosticCapture").is_none() {
        frame.payload = capture_payload(&frame.payload);
    }
    if let Some(error) = &mut frame.error {
        let mut end = error.len().min(1024);
        while !error.is_char_boundary(end) {
            end -= 1;
        }
        error.truncate(end);
    }
    frame.id = DEBUG_FRAME_ID.fetch_add(1, Ordering::Relaxed);
    // No second in-memory copy: all readers use the bounded on-disk store.
    let worker = DEBUG_WORKER.get_or_init(|| {
        worker::Worker::new(128, persist_debug_frame)
            .map_err(|error| eprintln!("debug capture worker unavailable: {error}"))
            .ok()
    });
    let rejected = match worker {
        Some(worker) => worker.submit(frame).err(),
        None => Some(frame),
    };
    if rejected
        .as_ref()
        .is_some_and(crate::services::connection_history_store::is_completed_connection_frame)
    {
        crate::services::connection_history_store::record_write_failure();
    }
}

fn persist_debug_frame(persisted: DebugFrame, dropped: u64) {
    if dropped > 0 {
        crate::services::file_logger::line(&format!(
            "debug capture skipped {dropped} frames because its bounded worker was unavailable or busy"
        ));
    }
    let _ = crate::services::debug_store::append(&persisted);
    if crate::services::connection_history_store::append_if_completed(&persisted).is_err() {
        crate::services::connection_history_store::record_write_failure();
    }
    if let Some(observer) = DEBUG_FRAME_OBSERVER.get() {
        // Diagnostics must never be able to break the IPC path.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| observer(&persisted)));
    }
}

// Retained for callers that clear diagnostics; there is no duplicated memory ring.
pub(crate) fn clear_debug_frames() {}

pub(crate) fn seed_next_debug_frame_id(next_id: u64) {
    let mut current = DEBUG_FRAME_ID.load(Ordering::SeqCst);
    while current < next_id {
        match DEBUG_FRAME_ID.compare_exchange(current, next_id, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}
