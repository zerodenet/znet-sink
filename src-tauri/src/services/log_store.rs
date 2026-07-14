use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::models::logs::{LogEntry, LogPage, LogQuery};

const LOGS_FILE: &str = "logs.jsonl";
const MIN_PERSISTED_LOG_ENTRIES: usize = 5_000;

pub(crate) fn load_recent(limit: usize) -> AppResult<Vec<LogEntry>> {
    load_recent_from_path(&logs_path()?, limit)
}

pub(crate) fn append(entry: &LogEntry) -> AppResult<()> {
    append_to_path(&logs_path()?, entry)
}

pub(crate) fn rotate(limit: usize) -> AppResult<()> {
    rotate_path(&logs_path()?, persisted_limit(limit))
}

pub(crate) fn clear() -> AppResult<()> {
    let path = logs_path()?;
    if !path.exists() {
        return Ok(());
    }

    fs::write(&path, "").map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to clear logs: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

pub(crate) fn query_page(query: &LogQuery) -> AppResult<LogPage> {
    query_page_from_path(&logs_path()?, query)
}

pub fn load_recent_from_path(path: &Path, limit: usize) -> AppResult<Vec<LogEntry>> {
    if limit == 0 || !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read logs: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to read logs: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                return Err(AppError {
                    code: "invalid_argument",
                    message: format!("failed to parse logs: {error}"),
                    details: Some(serde_json::json!({ "path": path.display().to_string() })),
                });
            }
        }
    }

    if entries.len() > limit {
        let remove_count = entries.len() - limit;
        entries.drain(0..remove_count);
    }

    Ok(entries)
}

pub fn query_page_from_path(path: &Path, query: &LogQuery) -> AppResult<LogPage> {
    let limit = query.limit.unwrap_or(200);
    if limit == 0 || !path.exists() {
        return Ok(LogPage {
            items: Vec::new(),
            has_more: false,
            oldest_available_id: None,
        });
    }

    let file = fs::File::open(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read logs: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    let reader = BufReader::new(file);
    let before_id = query.before_id.unwrap_or(u64::MAX);
    let mut oldest_available_id = None;
    let mut entries = VecDeque::with_capacity(limit);

    for line in reader.lines() {
        let line = line.map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to read logs: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry = serde_json::from_str::<LogEntry>(line).map_err(|error| AppError {
            code: "invalid_argument",
            message: format!("failed to parse logs: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;

        if query
            .source
            .as_ref()
            .is_some_and(|source| &entry.source != source)
        {
            continue;
        }
        if query
            .level
            .as_ref()
            .is_some_and(|level| &entry.level != level)
        {
            continue;
        }

        oldest_available_id.get_or_insert(entry.id);

        if entry.id >= before_id {
            continue;
        }

        if entries.len() == limit {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    let items = entries.into_iter().collect::<Vec<_>>();
    let has_more = match (oldest_available_id, items.first()) {
        (Some(oldest), Some(first)) => first.id > oldest,
        _ => false,
    };

    Ok(LogPage {
        items,
        has_more,
        oldest_available_id,
    })
}

pub fn append_to_path(path: &Path, entry: &LogEntry) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to create logs directory: {error}"),
            details: Some(serde_json::json!({ "path": parent.display().to_string() })),
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to open logs: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;

    let content = serde_json::to_string(entry).map_err(|error| AppError {
        code: "internal",
        message: format!("failed to serialize log entry: {error}"),
        details: None,
    })?;

    writeln!(file, "{content}").map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write logs: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

pub fn rotate_path(path: &Path, limit: usize) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }

    if limit == 0 {
        return fs::write(path, "").map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to rotate logs: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        });
    }

    let entries = load_recent_from_path(path, limit)?;
    let mut content = String::new();
    for entry in entries {
        let line = serde_json::to_string(&entry).map_err(|error| AppError {
            code: "internal",
            message: format!("failed to serialize log entry: {error}"),
            details: None,
        })?;
        content.push_str(&line);
        content.push('\n');
    }

    fs::write(path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to rotate logs: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn logs_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(LOGS_FILE))
}

fn persisted_limit(limit: usize) -> usize {
    limit.max(MIN_PERSISTED_LOG_ENTRIES)
}

#[cfg(test)]
mod tests {
    use crate::models::logs::{LogLevel, LogQuery, LogSource};

    use super::*;

    #[test]
    fn log_store_appends_and_loads_recent_entries() {
        let dir = std::env::temp_dir().join(format!("znet-log-store-{}", std::process::id()));
        let path = dir.join("logs.jsonl");

        for id in 1..=3 {
            append_to_path(
                &path,
                &LogEntry {
                    id,
                    source: LogSource::App,
                    level: LogLevel::Info,
                    message: format!("entry-{id}"),
                    fields: None,
                    occurred_at_unix_ms: id,
                },
            )
            .unwrap();
        }

        let entries = load_recent_from_path(&path, 2).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, 2);
        assert_eq!(entries[1].id, 3);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn log_store_rotates_file_to_recent_entries() {
        let dir =
            std::env::temp_dir().join(format!("znet-log-store-rotate-{}", std::process::id()));
        let path = dir.join("logs.jsonl");

        for id in 1..=5 {
            append_to_path(
                &path,
                &LogEntry {
                    id,
                    source: LogSource::Core,
                    level: LogLevel::Error,
                    message: format!("entry-{id}"),
                    fields: None,
                    occurred_at_unix_ms: id,
                },
            )
            .unwrap();
        }

        rotate_path(&path, 3).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().count(), 3);

        let entries = load_recent_from_path(&path, 10).unwrap();
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![3, 4, 5]
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn log_store_queries_filtered_page() {
        let dir = std::env::temp_dir().join(format!("znet-log-query-{}", std::process::id()));
        let path = dir.join("logs.jsonl");

        for id in 1..=6 {
            append_to_path(
                &path,
                &LogEntry {
                    id,
                    source: if id % 2 == 0 {
                        LogSource::Core
                    } else {
                        LogSource::App
                    },
                    level: if id % 3 == 0 {
                        LogLevel::Error
                    } else {
                        LogLevel::Info
                    },
                    message: format!("entry-{id}"),
                    fields: None,
                    occurred_at_unix_ms: id,
                },
            )
            .unwrap();
        }

        let page = query_page_from_path(
            &path,
            &LogQuery {
                source: Some(LogSource::App),
                limit: Some(2),
                ..LogQuery::default()
            },
        )
        .unwrap();

        assert_eq!(
            page.items.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![3, 5]
        );
        assert!(page.has_more);
        assert_eq!(page.oldest_available_id, Some(1));

        let _ = fs::remove_dir_all(dir);
    }
}
