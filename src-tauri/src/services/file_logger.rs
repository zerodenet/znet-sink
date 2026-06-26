//! Persistent JSON-Lines file logger for early-startup and crash diagnostics.
//!
//! Unlike [`super::logs`] (which lives in `AppState` memory and therefore only
//! exists after the State phase), this logger opens a file at process start
//! and survives crashes — including a panic hook that writes the panic payload
//! before the process exits. Output is JSON Lines at
//! `<data_dir>/logs/gui.log.jsonl`, one record per line:
//!
//! ```jsonc
//! {"ts":1782440087604,"level":"info","source":"lifecycle","msg":"entering phase state"}
//! ```

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use serde_json::Value;

use crate::errors::AppError;
use crate::services::{common, data_dir};

static LOGGER: OnceLock<Mutex<BufWriter<File>>> = OnceLock::new();

#[derive(Serialize)]
struct Record<'a> {
    ts: u64,
    level: &'a str,
    source: &'a str,
    msg: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

/// Open `<data_dir>/logs/gui.log.jsonl` for append and install the panic hook.
///
/// Best-effort: on failure warns on stderr and continues — file logging is a
/// diagnostic aid, not a startup requirement, and must never block the app
/// from launching.
pub fn init() {
    match try_open() {
        Ok(path) => {
            emit(
                "info",
                "file_logger",
                "file logger initialized",
                Some(serde_json::json!({ "path": path.to_string_lossy() })),
            );
            install_panic_hook();
        }
        Err(error) => {
            eprintln!("[ZNet] file_logger: init failed ({})", error.message);
        }
    }
}

fn try_open() -> Result<std::path::PathBuf, AppError> {
    let path = data_dir()?.join("logs").join("gui.log.jsonl");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::internal(format!("create log dir: {e}")))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| AppError::internal(format!("open log file: {e}")))?;
    let _ = LOGGER.set(Mutex::new(BufWriter::new(file)));
    Ok(path)
}

fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<non-string panic payload>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        emit(
            "error",
            "panic",
            msg,
            Some(serde_json::json!({ "location": location, "thread": thread })),
        );
        default(info);
    }));
}

/// Append one JSON record. Best-effort: silently no-ops if [`init`] failed.
pub fn emit(level: &str, source: &str, msg: &str, details: Option<Value>) {
    let record = Record {
        ts: common::now_unix_ms(),
        level,
        source,
        msg,
        details,
    };
    let line = match serde_json::to_string(&record) {
        Ok(s) => s,
        Err(_) => return,
    };
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut w) = logger.lock() {
            let _ = writeln!(w, "{line}");
            let _ = w.flush();
        }
    }
}

/// `eprintln!("[ZNet] {message}")` + file log. Preserves the existing console
/// output while also persisting the line for post-crash inspection. Use this
/// to replace bare `eprintln!("[ZNet] …")` calls in startup/shutdown paths.
pub fn line(message: &str) {
    eprintln!("[ZNet] {message}");
    emit("info", "app", message, None);
}
