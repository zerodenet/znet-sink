//! Byte-bounded IPC segments. Appends rotate files, never parse history.
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::models::debug::{DebugFrame, DebugFramePage, DebugFrameQuery};

pub(crate) const SEGMENT_BYTES: u64 = 1024 * 1024;
const ARCHIVE_COUNT: usize = 4;
const RECORD_BYTES: usize = 64 * 1024;
const PAGE_BYTES: usize = 512 * 1024;
static DEBUG_FILE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn io_error(error: std::io::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!("IPC diagnostic storage: {error}"),
        details: None,
    }
}

pub(crate) fn query_page(query: &DebugFrameQuery) -> AppResult<DebugFramePage> {
    let _guard = DEBUG_FILE_LOCK.lock().expect("debug file mutex poisoned");
    query_page_from_path(&debug_path()?, query)
}

pub(crate) fn append(frame: &DebugFrame) -> AppResult<()> {
    let _guard = DEBUG_FILE_LOCK.lock().expect("debug file mutex poisoned");
    append_to_path(&debug_path()?, frame)
}

pub(crate) fn clear() -> AppResult<()> {
    let _guard = DEBUG_FILE_LOCK.lock().expect("debug file mutex poisoned");
    clear_path(&debug_path()?)
}

fn clear_path(path: &Path) -> AppResult<()> {
    for path in segment_paths(path) {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

pub(crate) fn rotate() -> AppResult<()> {
    let _guard = DEBUG_FILE_LOCK.lock().expect("debug file mutex poisoned");
    // One bounded tail read migrates oversized logs from older clients. No JSON
    // parsing, and no enormous historical payload enters the allocator.
    for path in segment_paths(&debug_path()?) {
        bound_legacy_file(&path)?;
    }
    Ok(())
}

fn bound_legacy_file(path: &Path) -> AppResult<()> {
    let size = match fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(error)),
    };
    if size <= SEGMENT_BYTES {
        return Ok(());
    }
    let mut file = fs::File::open(path).map_err(io_error)?;
    file.seek(SeekFrom::Start(size - SEGMENT_BYTES))
        .map_err(io_error)?;
    let mut tail = Vec::with_capacity(SEGMENT_BYTES as usize);
    file.take(SEGMENT_BYTES)
        .read_to_end(&mut tail)
        .map_err(io_error)?;
    let start = tail
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(tail.len(), |index| index + 1);
    fs::write(path, &tail[start..]).map_err(io_error)
}

pub(crate) fn latest_id() -> AppResult<Option<u64>> {
    let _guard = DEBUG_FILE_LOCK.lock().expect("debug file mutex poisoned");
    latest_id_from_path(&debug_path()?)
}

pub(crate) fn segment_paths(path: &Path) -> Vec<PathBuf> {
    (1..=ARCHIVE_COUNT)
        .rev()
        .map(|index| archive_path(path, index))
        .chain(std::iter::once(path.to_path_buf()))
        .collect()
}

fn archive_path(path: &Path, index: usize) -> PathBuf {
    path.with_file_name(format!(
        "{}.{index}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn rotate_path(path: &Path) -> AppResult<()> {
    let oldest = archive_path(path, ARCHIVE_COUNT);
    if oldest.exists() {
        fs::remove_file(oldest).map_err(io_error)?;
    }
    for index in (1..ARCHIVE_COUNT).rev() {
        let source = archive_path(path, index);
        if source.exists() {
            fs::rename(source, archive_path(path, index + 1)).map_err(io_error)?;
        }
    }
    if path.exists() {
        fs::rename(path, archive_path(path, 1)).map_err(io_error)?;
    }
    Ok(())
}

pub(crate) fn append_to_path(path: &Path, frame: &DebugFrame) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    // Stop serialization at the byte limit, including for accidental unbounded
    // callers. Checking only after to_vec would already allocate the huge record.
    let mut content = RecordWriter(Vec::with_capacity(1024));
    serde_json::to_writer(&mut content, frame).map_err(|error| {
        AppError::invalid_argument(format!(
            "IPC diagnostic record limit/serialization: {error}"
        ))
    })?;
    let content = content.0;
    let size = match fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(io_error(error)),
    };
    if size > SEGMENT_BYTES {
        bound_legacy_file(path)?;
    }
    if size + content.len() as u64 + 1 > SEGMENT_BYTES {
        rotate_path(path)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(&content)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(io_error)
}

struct RecordWriter(Vec<u8>);
impl Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) >= RECORD_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "record exceeds 64 KiB",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Skip oversized legacy/malformed lines without allocating proportional to them.
fn visit_lines(path: &Path, mut visit: impl FnMut(&[u8])) -> AppResult<()> {
    for segment in segment_paths(path) {
        let file = match fs::File::open(segment) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_error(error)),
        };
        let mut reader = BufReader::new(file);
        let mut line = Vec::with_capacity(1024);
        let mut oversized = false;
        loop {
            let chunk = reader.fill_buf().map_err(io_error)?;
            if chunk.is_empty() {
                if !oversized && !line.is_empty() {
                    visit(&line);
                }
                break;
            }
            let end = chunk
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|index| index + 1);
            let length = end.unwrap_or(chunk.len());
            if !oversized && line.len() + length <= RECORD_BYTES {
                line.extend_from_slice(&chunk[..length]);
            } else {
                oversized = true;
                line.clear();
            }
            reader.consume(length);
            if end.is_some() {
                if !oversized {
                    visit(&line);
                }
                line.clear();
                oversized = false;
            }
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    id: u64,
    frame_type: String,
}

pub(crate) fn query_page_from_path(
    path: &Path,
    query: &DebugFrameQuery,
) -> AppResult<DebugFramePage> {
    let limit = query.limit.unwrap_or(200).min(1000);
    let before_id = query.before_id.unwrap_or(u64::MAX);
    let frame_type = query
        .frame_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut items = VecDeque::new();
    let mut bytes = 0;
    let mut oldest_available_id: Option<u64> = None;
    let mut has_more = false;
    visit_lines(path, |line| {
        let Ok(header) = serde_json::from_slice::<Header>(line) else {
            return;
        };
        if frame_type.is_some_and(|expected| header.frame_type != expected) {
            return;
        }
        oldest_available_id = Some(oldest_available_id.map_or(header.id, |id| id.min(header.id)));
        if header.id >= before_id || limit == 0 {
            return;
        }
        let Ok(frame) = serde_json::from_slice::<DebugFrame>(line) else {
            return;
        };
        while items.len() >= limit || bytes + line.len() > PAGE_BYTES {
            let Some((_, length)) = items.pop_front() else {
                break;
            };
            bytes -= length;
            has_more = true;
        }
        bytes += line.len();
        items.push_back((frame, line.len()));
    })?;
    Ok(DebugFramePage {
        items: items.into_iter().map(|(frame, _)| frame).collect(),
        has_more,
        oldest_available_id,
        history: None,
    })
}

fn latest_id_from_path(path: &Path) -> AppResult<Option<u64>> {
    let mut latest: Option<u64> = None;
    visit_lines(path, |line| {
        if let Ok(header) = serde_json::from_slice::<Header>(line) {
            latest = Some(latest.map_or(header.id, |id| id.max(header.id)));
        }
    })?;
    Ok(latest)
}

fn debug_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("logs").join("debug.log.jsonl"))
}

#[cfg(test)]
#[path = "debug_store_tests.rs"]
mod tests;
