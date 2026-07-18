use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::models::debug::{DebugFrame, DebugFramePage, DebugFrameQuery};

const DEBUG_LOG_DIR: &str = "logs";
const DEBUG_LOG_FILE: &str = "debug.log.jsonl";
const DEBUG_PERSISTED_LIMIT: usize = 5_000;
const DEBUG_ROTATE_EVERY: u64 = 100;

pub(crate) fn query_page(query: &DebugFrameQuery) -> AppResult<DebugFramePage> {
    query_page_from_path(&debug_path()?, query)
}

pub(crate) fn append(frame: &DebugFrame) -> AppResult<()> {
    append_to_path(&debug_path()?, frame)?;
    if frame.id.is_multiple_of(DEBUG_ROTATE_EVERY) {
        rotate()?;
    }
    Ok(())
}

pub(crate) fn clear() -> AppResult<()> {
    let path = debug_path()?;
    if !path.exists() {
        return Ok(());
    }

    fs::write(&path, "").map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to clear debug frames: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

pub(crate) fn rotate() -> AppResult<()> {
    rotate_path(&debug_path()?, DEBUG_PERSISTED_LIMIT)
}

pub(crate) fn latest_id() -> AppResult<Option<u64>> {
    let page = query_page_from_path(
        &debug_path()?,
        &DebugFrameQuery {
            limit: Some(1),
            ..DebugFrameQuery::default()
        },
    )?;
    Ok(page.items.last().map(|frame| frame.id))
}

pub(crate) fn query_page_from_path(
    path: &Path,
    query: &DebugFrameQuery,
) -> AppResult<DebugFramePage> {
    let limit = query.limit.unwrap_or(200);
    if limit == 0 || !path.exists() {
        return Ok(DebugFramePage {
            items: Vec::new(),
            has_more: false,
            oldest_available_id: None,
        });
    }

    let file = fs::File::open(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read debug frames: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;
    let reader = BufReader::new(file);
    let before_id = query.before_id.unwrap_or(u64::MAX);
    let frame_type = query
        .frame_type
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    let mut oldest_available_id = None;
    let mut items = VecDeque::with_capacity(limit);

    for line in reader.lines() {
        let line = line.map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to read debug frames: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(frame) = serde_json::from_str::<DebugFrame>(line) else {
            // A partial final write or an older incompatible record must not
            // make every valid debug frame in the history unreadable.
            continue;
        };

        if frame_type.is_some_and(|expected| frame.frame_type != expected) {
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

pub(crate) fn append_to_path(path: &Path, frame: &DebugFrame) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to create debug log directory: {error}"),
            details: Some(serde_json::json!({ "path": parent.display().to_string() })),
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to open debug log: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        })?;

    let content = serde_json::to_string(frame).map_err(|error| AppError {
        code: "internal",
        message: format!("failed to serialize debug frame: {error}"),
        details: None,
    })?;

    writeln!(file, "{content}").map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write debug log: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

pub(crate) fn load_recent_from_path(path: &Path, limit: usize) -> AppResult<Vec<DebugFrame>> {
    let page = query_page_from_path(
        path,
        &DebugFrameQuery {
            limit: Some(limit),
            ..DebugFrameQuery::default()
        },
    )?;
    Ok(page.items)
}

pub(crate) fn rotate_path(path: &Path, limit: usize) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }

    if limit == 0 {
        return fs::write(path, "").map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to rotate debug frames: {error}"),
            details: Some(serde_json::json!({ "path": path.display().to_string() })),
        });
    }

    let frames = load_recent_from_path(path, limit)?;
    let mut content = String::new();
    for frame in frames {
        let line = serde_json::to_string(&frame).map_err(|error| AppError {
            code: "internal",
            message: format!("failed to serialize debug frame: {error}"),
            details: None,
        })?;
        content.push_str(&line);
        content.push('\n');
    }

    fs::write(path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to rotate debug frames: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn debug_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(DEBUG_LOG_DIR).join(DEBUG_LOG_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: u64, frame_type: &str) -> DebugFrame {
        DebugFrame {
            id,
            at_ms: id,
            direction: "tx".to_string(),
            frame_type: frame_type.to_string(),
            payload: serde_json::json!({ "id": id }),
            elapsed_ms: None,
            error: None,
        }
    }

    #[test]
    fn debug_store_queries_recent_page() {
        let dir = std::env::temp_dir().join(format!("znet-debug-store-{}", std::process::id()));
        let path = dir.join("debug.log.jsonl");

        for id in 1..=5 {
            append_to_path(
                &path,
                &frame(id, if id % 2 == 0 { "event" } else { "query" }),
            )
            .unwrap();
        }

        let page = query_page_from_path(
            &path,
            &DebugFrameQuery {
                frame_type: Some("query".to_string()),
                limit: Some(2),
                ..DebugFrameQuery::default()
            },
        )
        .unwrap();

        assert_eq!(
            page.items.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![3, 5]
        );
        assert!(page.has_more);
        assert_eq!(page.oldest_available_id, Some(1));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn debug_store_rotates_to_recent_frames() {
        let dir = std::env::temp_dir().join(format!("znet-debug-rotate-{}", std::process::id()));
        let path = dir.join("debug.log.jsonl");

        for id in 1..=4 {
            append_to_path(&path, &frame(id, "event")).unwrap();
        }

        rotate_path(&path, 2).unwrap();

        let frames = load_recent_from_path(&path, 10).unwrap();
        assert_eq!(
            frames.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![3, 4]
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn debug_store_skips_malformed_records() {
        let dir = std::env::temp_dir().join(format!("znet-debug-malformed-{}", std::process::id()));
        let path = dir.join("debug.log.jsonl");

        append_to_path(&path, &frame(1, "query")).unwrap();
        fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n",
                serde_json::to_string(&frame(1, "query")).unwrap(),
                serde_json::to_string(&frame(2, "event")).unwrap()
            ),
        )
        .unwrap();

        let frames = load_recent_from_path(&path, 10).unwrap();
        assert_eq!(
            frames.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![1, 2]
        );

        let _ = fs::remove_dir_all(dir);
    }
}
