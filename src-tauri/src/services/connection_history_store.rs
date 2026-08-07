use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::models::debug::{DebugFrame, DebugFramePage, DebugFrameQuery};

const HISTORY_LOG_DIR: &str = "logs";
const HISTORY_LOG_FILE: &str = "connection-history.log.jsonl";
const HISTORY_RECORD_LIMIT: usize = 10_000;
const HISTORY_MAX_BYTES: u64 = 32 * 1024 * 1024;
const HISTORY_MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
const HISTORY_ROTATE_EVERY: u64 = 100;

static HISTORY_FILE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn append_if_completed(frame: &DebugFrame) -> AppResult<()> {
    if !is_completed_connection_frame(frame) {
        return Ok(());
    }

    let _guard = HISTORY_FILE_LOCK
        .lock()
        .expect("connection history file mutex poisoned");
    let path = history_path()?;
    append_to_path(&path, frame)?;

    let over_byte_limit = fs::metadata(&path)
        .map(|metadata| metadata.len() > HISTORY_MAX_BYTES)
        .unwrap_or(false);
    if frame.id.is_multiple_of(HISTORY_ROTATE_EVERY) || over_byte_limit {
        rotate_path(&path)?;
    }
    Ok(())
}

pub(crate) fn query_page(query: &DebugFrameQuery) -> AppResult<DebugFramePage> {
    let _guard = HISTORY_FILE_LOCK
        .lock()
        .expect("connection history file mutex poisoned");
    query_page_from_path(&history_path()?, query)
}

pub(crate) fn clear() -> AppResult<()> {
    let _guard = HISTORY_FILE_LOCK
        .lock()
        .expect("connection history file mutex poisoned");
    let path = history_path()?;
    if !path.exists() {
        return Ok(());
    }

    fs::write(&path, "").map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to clear connection history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn query_page_from_path(path: &Path, query: &DebugFrameQuery) -> AppResult<DebugFramePage> {
    let limit = query.limit.unwrap_or(50);
    if limit == 0 || !path.exists() {
        return Ok(DebugFramePage {
            items: Vec::new(),
            has_more: false,
            oldest_available_id: None,
        });
    }

    let file = fs::File::open(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read connection history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    let reader = BufReader::new(file);
    let before_id = query.before_id.unwrap_or(u64::MAX);
    let mut oldest_available_id = None;
    let mut items = VecDeque::with_capacity(limit);

    for line in reader.lines() {
        let line = line.map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to read connection history: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
        let Ok(frame) = serde_json::from_str::<DebugFrame>(line.trim()) else {
            continue;
        };
        if !is_completed_connection_frame(&frame) {
            continue;
        }

        oldest_available_id.get_or_insert(frame.id);
        if frame.id >= before_id {
            continue;
        }
        if items.len() == limit {
            items.pop_front();
        }
        items.push_back(frame);
    }

    let items = items.into_iter().collect::<Vec<_>>();
    let has_more = match (oldest_available_id, items.first()) {
        (Some(oldest), Some(first)) => first.id > oldest,
        _ => false,
    };

    Ok(DebugFramePage {
        items,
        has_more,
        oldest_available_id,
    })
}

fn append_to_path(path: &Path, frame: &DebugFrame) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to create connection history directory: {error}"),
            details: Some(serde_json::json!({ "path": parent.display().to_string() })),
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to open connection history: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
    let content = serde_json::to_string(frame).map_err(|error| AppError {
        code: "internal",
        message: format!("failed to serialize connection history record: {error}"),
        details: None,
    })?;
    writeln!(file, "{content}").map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write connection history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn rotate_path(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }

    let file = fs::File::open(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to rotate connection history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    let reader = BufReader::new(file);
    let cutoff = now_unix_ms().saturating_sub(HISTORY_MAX_AGE_MS);
    let mut records: VecDeque<(String, usize)> = VecDeque::new();
    let mut retained_bytes = 0usize;

    for line in reader.lines() {
        let line = line.map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to rotate connection history: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
        let Ok(frame) = serde_json::from_str::<DebugFrame>(line.trim()) else {
            continue;
        };
        if frame.at_ms < cutoff || !is_completed_connection_frame(&frame) {
            continue;
        }

        let serialized = serde_json::to_string(&frame).map_err(|error| AppError {
            code: "internal",
            message: format!("failed to serialize connection history record: {error}"),
            details: None,
        })?;
        let bytes = serialized.len() + 1;
        retained_bytes += bytes;
        records.push_back((serialized, bytes));

        while records.len() > HISTORY_RECORD_LIMIT
            || retained_bytes as u64 > HISTORY_MAX_BYTES
        {
            if let Some((_, removed_bytes)) = records.pop_front() {
                retained_bytes = retained_bytes.saturating_sub(removed_bytes);
            } else {
                break;
            }
        }
    }

    let mut content = String::with_capacity(retained_bytes);
    for (record, _) in records {
        content.push_str(&record);
        content.push('\n');
    }
    fs::write(path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to rotate connection history: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn is_completed_connection_frame(frame: &DebugFrame) -> bool {
    if frame.frame_type != "event" {
        return false;
    }
    let Some(envelope) = frame.payload.as_object() else {
        return false;
    };
    let event_type = envelope
        .get("event_type")
        .or_else(|| envelope.get("eventType"))
        .or_else(|| envelope.get("type"))
        .and_then(serde_json::Value::as_str);
    matches!(event_type, Some("flow.completed" | "connection.closed"))
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn history_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(HISTORY_LOG_DIR).join(HISTORY_LOG_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_frame(id: u64, at_ms: u64) -> DebugFrame {
        DebugFrame {
            id,
            at_ms,
            direction: "rx".to_string(),
            frame_type: "event".to_string(),
            payload: serde_json::json!({
                "eventType": "connection.closed",
                "payload": { "flowId": format!("flow-{id}") }
            }),
            elapsed_ms: None,
            error: None,
        }
    }

    #[test]
    fn detects_only_completed_connection_events() {
        let completed = completed_frame(1, now_unix_ms());
        assert!(is_completed_connection_frame(&completed));

        let mut updated = completed.clone();
        updated.payload["eventType"] = serde_json::json!("connection.updated");
        assert!(!is_completed_connection_frame(&updated));
    }
}
