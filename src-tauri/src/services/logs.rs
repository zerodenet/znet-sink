use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Mutex, OnceLock},
};

use crate::errors::AppResult;
use crate::models::logs::{LogAppend, LogEntry, LogLevel, LogPage, LogQuery, LogSource};
use crate::services::common::{lock, normalize_required, now_unix_ms};
use crate::services::log_store;
use crate::state::app_state::AppState;

/// Minimum log level for stderr output, controlled by `ZNET_LOG` env var.
static MIN_STDERR_LEVEL: OnceLock<LogLevel> = OnceLock::new();
static PLUGIN_HOST_EVENT_TIMES: OnceLock<Mutex<BTreeMap<String, VecDeque<u64>>>> = OnceLock::new();

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
    LEVEL_ORDER.iter().position(|l| l == level) <= LEVEL_ORDER.iter().position(|l| l == min)
}

/// Write a log entry visible in production.
///
/// - Always writes to stderr if `level` meets the `ZNET_LOG` threshold.
/// - If `state` is provided, also writes to the in-memory log buffer
///   (visible in the frontend LogPanel).
pub(crate) fn znet_log(state: Option<&AppState>, level: LogLevel, message: impl Into<String>) {
    znet_log_with_fields(state, level, message.into(), None);
}

pub(crate) fn znet_log_fields(
    state: Option<&AppState>,
    level: LogLevel,
    message: impl Into<String>,
    fields: serde_json::Value,
) {
    znet_log_with_fields(state, level, message.into(), Some(fields));
}

fn znet_log_with_fields(
    state: Option<&AppState>,
    level: LogLevel,
    msg: String,
    fields: Option<serde_json::Value>,
) {
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
        let _ = append_entry(state, LogSource::App, level, msg, fields);
    }
}

pub fn list(query: Option<LogQuery>) -> AppResult<LogPage> {
    let mut query = query.unwrap_or_default();
    query.limit = Some(query.limit.unwrap_or(200).clamp(1, 1_000));
    log_store::query_page(&query)
}

pub fn append(state: &AppState, input: LogAppend) -> AppResult<LogEntry> {
    let message = normalize_required(input.message, "message")?;
    append_entry(state, input.source, input.level, message, input.fields)
}

pub(crate) fn append_entry(
    state: &AppState,
    source: LogSource,
    level: LogLevel,
    message: String,
    fields: Option<serde_json::Value>,
) -> AppResult<LogEntry> {
    let message = crate::kernel::redaction::text(&message);
    let fields = fields.map(|value| crate::kernel::redaction::sensitive(&value));
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
    drop(entries);
    log_store::append(&entry, max_entries)?;

    Ok(entry)
}

/// Host-owned plugin execution telemetry. Never accepts a guest-provided source
/// or identity, and never records invocation payloads or SDK arguments.
pub(crate) fn plugin_host_event(
    state: &AppState,
    plugin_id: &str,
    component_id: &str,
    action: &str,
    result: Result<(), &crate::errors::AppError>,
) {
    if plugin_id.is_empty()
        || component_id.is_empty()
        || plugin_id.len() > 128
        || component_id.len() > 128
        || action.len() > 128
    {
        return;
    }
    let now = now_unix_ms();
    let owner = format!("{plugin_id}/{component_id}");
    let mut guard = PLUGIN_HOST_EVENT_TIMES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap();
    guard.retain(|_, times| {
        while times
            .front()
            .is_some_and(|time| now.saturating_sub(*time) >= 60_000)
        {
            times.pop_front();
        }
        !times.is_empty()
    });
    let times = guard.entry(owner).or_default();
    if times.len() >= 120 {
        return;
    }
    times.push_back(now);
    drop(guard);
    let (level, outcome, code) = match result {
        Ok(()) => (LogLevel::Info, "ok", None),
        Err(error) => (LogLevel::Error, "failed", Some(error.code)),
    };
    let _ = append_entry(
        state,
        LogSource::Plugin,
        level,
        format!("插件操作 {action} {outcome}"),
        Some(serde_json::json!({
            "pluginId": plugin_id,
            "componentId": component_id,
            "action": action,
            "outcome": outcome,
            "errorCode": code,
            "origin": "host",
        })),
    );
}

pub fn clear(state: &AppState) -> AppResult<()> {
    clear_memory(state)?;
    log_store::clear()?;
    Ok(())
}

pub(crate) fn clear_memory(state: &AppState) -> AppResult<()> {
    lock(state.logs(), "logs")?.clear();
    Ok(())
}
