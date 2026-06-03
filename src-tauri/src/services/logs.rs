use std::sync::OnceLock;

use tauri::State;

use crate::errors::AppResult;
use crate::models::logs::{LogAppend, LogEntry, LogLevel, LogQuery, LogSource};
use crate::services::common::{lock, normalize_required, now_unix_ms};
use crate::services::log_store;
use crate::state::app_state::AppState;

/// Minimum log level for stderr output, controlled by `ZNET_LOG` env var.
static MIN_STDERR_LEVEL: OnceLock<LogLevel> = OnceLock::new();

fn stderr_level() -> LogLevel {
    MIN_STDERR_LEVEL
        .get_or_init(|| {
            std::env::var("ZNET_LOG")
                .ok()
                .and_then(|v| match v.to_ascii_lowercase().as_str() {
                    "trace" => Some(LogLevel::Trace),
                    "debug" => Some(LogLevel::Debug),
                    "info" => Some(LogLevel::Info),
                    "warn" => Some(LogLevel::Warn),
                    "error" => Some(LogLevel::Error),
                    _ => None,
                })
                .unwrap_or(LogLevel::Info)
        })
        .clone()
}

const LEVEL_ORDER: &[LogLevel] = &[
    LogLevel::Error,
    LogLevel::Warn,
    LogLevel::Info,
    LogLevel::Debug,
    LogLevel::Trace,
];

fn level_meets(level: &LogLevel, min: &LogLevel) -> bool {
    LEVEL_ORDER.iter().position(|l| l == level)
        <= LEVEL_ORDER.iter().position(|l| l == min)
}

/// Write a log entry visible in production.
///
/// - Always writes to stderr if `level` meets the `ZNET_LOG` threshold.
/// - If `state` is provided, also writes to the in-memory log buffer
///   (visible in the frontend LogPanel).
pub(crate) fn znet_log(
    state: Option<&AppState>,
    level: LogLevel,
    message: impl Into<String>,
) {
    let msg: String = message.into();
    let min = stderr_level();

    if level_meets(&level, &min) {
        let prefix = match level {
            LogLevel::Error => "[ZNet] ERROR",
            LogLevel::Warn => "[ZNet] WARN",
            LogLevel::Info => "[ZNet]",
            LogLevel::Debug => "[ZNet] DEBUG",
            LogLevel::Trace => "[ZNet] TRACE",
        };
        eprintln!("{prefix} {msg}");
    }

    if let Some(state) = state {
        let _ = append_entry(state, LogSource::App, level, msg, None);
    }
}

pub fn list(state: State<'_, AppState>, query: Option<LogQuery>) -> AppResult<Vec<LogEntry>> {
    let query = query.unwrap_or_default();
    let limit = query.limit.unwrap_or(200).min(1000);
    let entries = lock(state.logs(), "logs")?;

    let mut result = entries
        .iter()
        .rev()
        .filter(|entry| {
            query
                .source
                .as_ref()
                .is_none_or(|source| &entry.source == source)
        })
        .filter(|entry| {
            query
                .level
                .as_ref()
                .is_none_or(|level| &entry.level == level)
        })
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    result.reverse();

    Ok(result)
}

pub fn append(state: State<'_, AppState>, input: LogAppend) -> AppResult<LogEntry> {
    let message = normalize_required(input.message, "message")?;
    append_entry(
        state.inner(),
        input.source,
        input.level,
        message,
        input.fields,
    )
}

pub(crate) fn append_entry(
    state: &AppState,
    source: LogSource,
    level: LogLevel,
    message: String,
    fields: Option<serde_json::Value>,
) -> AppResult<LogEntry> {
    let entry = LogEntry {
        id: state.next_record_id(),
        source,
        level,
        message,
        fields,
        occurred_at_unix_ms: now_unix_ms(),
    };

    let max_entries = lock(state.app_config(), "app_config")?.logs.max_entries;
    let mut entries = lock(state.logs(), "logs")?;
    entries.push(entry.clone());
    if entries.len() > max_entries {
        let remove_count = entries.len() - max_entries;
        entries.drain(0..remove_count);
    }
    log_store::append(&entry)?;
    log_store::rotate(max_entries)?;

    Ok(entry)
}

pub fn clear(state: State<'_, AppState>) -> AppResult<()> {
    lock(state.logs(), "logs")?.clear();
    log_store::clear()?;
    Ok(())
}
