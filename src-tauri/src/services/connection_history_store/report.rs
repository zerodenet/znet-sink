use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use super::*;

static REMOVED: AtomicU64 = AtomicU64::new(0);
static WRITE_FAILURES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub retained_records: usize,
    pub matched_records: usize,
    pub retained_bytes: u64,
    pub oldest_captured_at_ms: Option<u64>,
    pub newest_captured_at_ms: Option<u64>,
    pub record_limit: usize,
    pub byte_limit: u64,
    pub max_age_ms: u64,
    pub removed_since_client_start: u64,
    pub write_failures_since_client_start: u64,
    // No event sequence inference: the subscribed stream contains other kinds
    // of events, and offline periods cannot be reconstructed from this file.
    pub completeness: &'static str,
}

impl HistorySummary {
    pub(super) fn empty() -> Self {
        Self {
            retained_records: 0,
            matched_records: 0,
            retained_bytes: 0,
            oldest_captured_at_ms: None,
            newest_captured_at_ms: None,
            record_limit: HISTORY_RECORD_LIMIT,
            byte_limit: HISTORY_MAX_BYTES,
            max_age_ms: HISTORY_MAX_AGE_MS,
            removed_since_client_start: REMOVED.load(Ordering::Relaxed),
            write_failures_since_client_start: WRITE_FAILURES.load(Ordering::Relaxed),
            completeness: "unknown",
        }
    }

    pub(super) fn observe(&mut self, frame: &DebugFrame) {
        self.retained_records += 1;
        self.oldest_captured_at_ms = Some(
            self.oldest_captured_at_ms
                .map_or(frame.at_ms, |v| v.min(frame.at_ms)),
        );
        self.newest_captured_at_ms = Some(
            self.newest_captured_at_ms
                .map_or(frame.at_ms, |v| v.max(frame.at_ms)),
        );
    }
}

pub(super) fn record_removals(count: u64) {
    REMOVED.fetch_add(count, Ordering::Relaxed);
}

pub(crate) fn record_write_failure() {
    WRITE_FAILURES.fetch_add(1, Ordering::Relaxed);
}

// The debug journal has a shorter retention period and may have been cleaned
// independently. Seed from both stores so history cursors remain monotonic.
pub(crate) fn latest_id() -> AppResult<Option<u64>> {
    let _guard = HISTORY_FILE_LOCK
        .lock()
        .expect("connection history file mutex poisoned");
    latest_id_from_path(&history_path()?)
}

pub(super) fn latest_id_from_path(path: &Path) -> AppResult<Option<u64>> {
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(path)
        .map_err(|e| AppError::internal(format!("read history identity: {e}")))?;
    let mut latest: Option<u64> = None;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| AppError::internal(format!("read history identity: {e}")))?;
        if let Ok(frame) = serde_json::from_str::<DebugFrame>(&line) {
            latest = Some(latest.map_or(frame.id, |id| id.max(frame.id)));
        }
    }
    Ok(latest)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryExport {
    pub path: String,
    pub records: usize,
}

pub(crate) fn export(mut query: DebugFrameQuery) -> AppResult<HistoryExport> {
    query.frame_type = None;
    query.before_id = None;
    query.limit = Some(HISTORY_RECORD_LIMIT);
    // One bounded, consistent local read, independent of the rendered page.
    crate::services::diagnostic_storage::with_storage_lock(|| {
        let page = query_page(&query)?;
        let directory = data_dir()?.join("diagnostics").join(format!(
            "znet-sink-diagnostics-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        export_page(page, query, &directory)
    })
}

pub(super) fn export_page(
    page: DebugFramePage,
    query: DebugFrameQuery,
    directory: &Path,
) -> AppResult<HistoryExport> {
    fs::create_dir_all(directory)
        .map_err(|e| AppError::internal(format!("create history export directory: {e}")))?;
    let path = directory.join("connection-records.json");
    let value = serde_json::json!({
        "schema": "znet.connection-records.v1",
        "exportedAtUnixMs": now_unix_ms(),
        "scope": "locally_received_completed_flow_events",
        "filters": query,
        "summary": page.history,
        "hasMore": page.has_more,
        "limitations": [
            "Only locally received completed-flow metadata; not packet capture or HTTP bodies.",
            "Offline, retention, and write failures may leave gaps; missing record counts are unknown.",
            "Capture timestamps are client receipt times, not connection start times.",
            "Records may include destinations, source addresses, process paths, and routing details."
        ],
        "records": page.items,
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| AppError::internal(format!("create history export: {e}")))?;
    if let Err(error) = serde_json::to_writer_pretty(&mut file, &value) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(AppError::internal(format!("write history export: {error}")));
    }
    Ok(HistoryExport {
        path: path.to_string_lossy().into_owned(),
        records: page.items.len(),
    })
}
