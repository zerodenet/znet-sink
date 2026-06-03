use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::models::logs::LogEntry;

const LOGS_FILE: &str = "logs.jsonl";

pub(crate) fn load_recent(limit: usize) -> AppResult<Vec<LogEntry>> {
    load_recent_from_path(&logs_path()?, limit)
}

pub(crate) fn append(entry: &LogEntry) -> AppResult<()> {
    append_to_path(&logs_path()?, entry)
}

pub(crate) fn rotate(limit: usize) -> AppResult<()> {
    rotate_path(&logs_path()?, limit)
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


#[cfg(test)]
mod tests {
    use crate::models::logs::{LogLevel, LogSource};

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
}
