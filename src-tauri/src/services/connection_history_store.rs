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
        if !is_completed_connection_frame(&frame) || !matches_query(&frame, query) {
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

fn matches_query(frame: &DebugFrame, query: &DebugFrameQuery) -> bool {
    let Some(envelope) = frame.payload.as_object() else {
        return false;
    };
    let payload = envelope
        .get("payload")
        .and_then(serde_json::Value::as_object)
        .unwrap_or(envelope);
    let record = payload
        .get("record")
        .and_then(serde_json::Value::as_object)
        .unwrap_or(payload);

    if let Some(search) = normalized_filter(query.search.as_deref()) {
        let haystack = serde_json::to_string(record)
            .unwrap_or_default()
            .to_lowercase();
        if !haystack.contains(&search) {
            return false;
        }
    }

    if let Some(expected) = normalized_filter(query.protocol.as_deref()) {
        let actual = text(record, &["network", "protocol"])
            .map(str::to_lowercase)
            .unwrap_or_default();
        if actual != expected {
            return false;
        }
    }

    if let Some(expected) = normalized_filter(query.outbound.as_deref()) {
        let path = object(record.get("path"));
        let outbound = path
            .and_then(|value| object(value.get("outbound")))
            .or_else(|| object(record.get("outbound")));
        let actual = outbound
            .and_then(|value| text(value, &["tag"]))
            .or_else(|| text(record, &["outbound_tag", "outboundTag"]))
            .map(str::to_lowercase)
            .unwrap_or_default();
        if actual != expected {
            return false;
        }
    }

    if let Some(expected) = normalized_filter(query.outcome.as_deref()) {
        let result = object(record.get("result"));
        let actual = result
            .and_then(|value| text(value, &["outcome", "close_reason", "closeReason"]))
            .or_else(|| text(record, &["outcome", "close_reason", "closeReason"]))
            .map(str::to_lowercase)
            .unwrap_or_default();
        if actual != expected {
            return false;
        }
    }

    true
}

fn normalized_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "all")
        .map(str::to_lowercase)
}

fn object(value: Option<&serde_json::Value>) -> Option<&serde_json::Map<String, serde_json::Value>> {
    value.and_then(serde_json::Value::as_object)
}

fn text<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
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
                "payload": {
                    "record": {
                        "flowId": format!("flow-{id}"),
                        "network": "tcp",
                        "path": { "outbound": { "tag": "proxy-a" } },
                        "result": { "outcome": "success" }
                    }
                }
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

    #[test]
    fn filters_connection_history_before_paging() {
        let frame = completed_frame(1, now_unix_ms());
        let query = DebugFrameQuery {
            protocol: Some("tcp".to_string()),
            outbound: Some("proxy-a".to_string()),
            outcome: Some("success".to_string()),
            search: Some("flow-1".to_string()),
            ..DebugFrameQuery::default()
        };
        assert!(matches_query(&frame, &query));
    }
}
