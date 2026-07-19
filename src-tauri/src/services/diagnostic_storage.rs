use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;

use super::{core_config, data_dir, debug_store, file_logger, logs};
use crate::errors::{AppError, AppResult};
use crate::models::debug::clear_debug_frames;
use crate::state::app_state::AppState;

const DIAGNOSTICS_DIR: &str = "diagnostics";
const DIAGNOSTIC_EXPORT_PREFIX: &str = "znet-sink-diagnostics-";

static STORAGE_OPERATION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStorageSummary {
    pub live_log_bytes: u64,
    pub live_log_file_count: usize,
    pub diagnostic_export_bytes: u64,
    pub diagnostic_export_count: usize,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStorageCleanupFailure {
    pub target: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStorageCleanupResult {
    pub bytes_reclaimed: u64,
    pub cleared_file_count: usize,
    pub removed_diagnostic_export_count: usize,
    pub remaining_bytes: u64,
    pub failures: Vec<DebugStorageCleanupFailure>,
}

/// Serialize diagnostic export, storage inspection, and cleanup so a cleanup
/// cannot remove a bundle while it is being written.
pub(crate) fn with_storage_lock<T>(operation: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    let _guard = STORAGE_OPERATION_LOCK
        .lock()
        .map_err(|_| AppError::internal("diagnostic storage mutex poisoned"))?;
    operation()
}

pub(crate) fn summary() -> AppResult<DebugStorageSummary> {
    with_storage_lock(|| {
        let data_dir = data_dir()?;
        let core_log_path = core_config::managed_core_log_path()?;
        summary_at(&data_dir, &core_log_path)
    })
}

pub(crate) fn clear(state: &AppState) -> AppResult<DebugStorageCleanupResult> {
    with_storage_lock(|| clear_locked(state))
}

pub(crate) fn diagnostic_log_paths() -> AppResult<[PathBuf; 4]> {
    let data_dir = data_dir()?;
    Ok(diagnostic_log_paths_at(
        &data_dir,
        core_config::managed_core_log_path()?,
    ))
}

pub(crate) fn diagnostic_log_paths_at(data_dir: &Path, core_log_path: PathBuf) -> [PathBuf; 4] {
    [
        data_dir.join("logs").join("gui.log.jsonl"),
        data_dir.join("logs.jsonl"),
        data_dir.join("logs").join("debug.log.jsonl"),
        core_log_path,
    ]
}

fn clear_locked(state: &AppState) -> AppResult<DebugStorageCleanupResult> {
    let data_dir = data_dir()?;
    let core_log_path = core_config::managed_core_log_path()?;
    let [gui_log_path, application_log_path, debug_log_path, core_log_path] =
        diagnostic_log_paths_at(&data_dir, core_log_path);
    let mut result = DebugStorageCleanupResult::default();

    match managed_file_size(&data_dir, &application_log_path) {
        Ok(application_log_size) => record_file_cleanup(
            &mut result,
            "application-log",
            application_log_size,
            logs::clear(state),
        ),
        Err(error) => {
            if let Err(memory_error) = logs::clear_memory(state) {
                record_cleanup_error(&mut result, "application-log-memory", memory_error);
            }
            record_cleanup_error(&mut result, "application-log", error);
        }
    }

    clear_debug_frames();
    match managed_file_size(&data_dir, &debug_log_path) {
        Ok(debug_log_size) => record_file_cleanup(
            &mut result,
            "ipc-debug-log",
            debug_log_size,
            debug_store::clear(),
        ),
        Err(error) => record_cleanup_error(&mut result, "ipc-debug-log", error),
    }

    match managed_file_size(&data_dir, &gui_log_path) {
        Ok(gui_log_size) => record_file_cleanup(
            &mut result,
            "gui-lifecycle-log",
            gui_log_size,
            file_logger::clear(),
        ),
        Err(error) => record_cleanup_error(&mut result, "gui-lifecycle-log", error),
    }

    match managed_file_size(&data_dir, &core_log_path) {
        Ok(core_log_size) => record_file_cleanup(
            &mut result,
            "kernel-log",
            core_log_size,
            truncate_file(&core_log_path),
        ),
        Err(error) => record_cleanup_error(&mut result, "kernel-log", error),
    }

    let diagnostics_root = data_dir.join(DIAGNOSTICS_DIR);
    match generated_export_directories(&diagnostics_root) {
        Ok(directories) => {
            for directory in directories {
                let size = match directory_size(&directory, &directory) {
                    Ok(size) => size,
                    Err(error) => {
                        result.failures.push(DebugStorageCleanupFailure {
                            target: "diagnostic-export".to_string(),
                            message: error.message,
                        });
                        continue;
                    }
                };
                match fs::remove_dir_all(&directory) {
                    Ok(()) => {
                        result.bytes_reclaimed = result.bytes_reclaimed.saturating_add(size);
                        result.removed_diagnostic_export_count += 1;
                    }
                    Err(error) => result.failures.push(DebugStorageCleanupFailure {
                        target: "diagnostic-export".to_string(),
                        message: format!("failed to remove diagnostic export: {error}"),
                    }),
                }
            }
        }
        Err(error) => result.failures.push(DebugStorageCleanupFailure {
            target: "diagnostic-exports".to_string(),
            message: error.message,
        }),
    }

    match summary_at(&data_dir, &core_log_path) {
        Ok(summary) => result.remaining_bytes = summary.total_bytes,
        Err(error) => result.failures.push(DebugStorageCleanupFailure {
            target: "storage-summary".to_string(),
            message: error.message,
        }),
    }
    Ok(result)
}

fn record_file_cleanup(
    result: &mut DebugStorageCleanupResult,
    target: &str,
    existing_size: Option<u64>,
    cleanup: AppResult<()>,
) {
    match cleanup {
        Ok(()) => {
            if let Some(size) = existing_size {
                result.bytes_reclaimed = result.bytes_reclaimed.saturating_add(size);
                result.cleared_file_count += 1;
            }
        }
        Err(error) => result.failures.push(DebugStorageCleanupFailure {
            target: target.to_string(),
            message: error.message,
        }),
    }
}

fn record_cleanup_error(result: &mut DebugStorageCleanupResult, target: &str, error: AppError) {
    result.failures.push(DebugStorageCleanupFailure {
        target: target.to_string(),
        message: error.message,
    });
}

fn summary_at(data_dir: &Path, core_log_path: &Path) -> AppResult<DebugStorageSummary> {
    let mut summary = DebugStorageSummary::default();
    for path in diagnostic_log_paths_at(data_dir, core_log_path.to_path_buf()) {
        if let Some(size) = managed_file_size(data_dir, &path)? {
            summary.live_log_bytes = summary.live_log_bytes.saturating_add(size);
            summary.live_log_file_count += 1;
        }
    }

    for directory in generated_export_directories(&data_dir.join(DIAGNOSTICS_DIR))? {
        summary.diagnostic_export_bytes = summary
            .diagnostic_export_bytes
            .saturating_add(directory_size(&directory, &directory)?);
        summary.diagnostic_export_count += 1;
    }
    summary.total_bytes = summary
        .live_log_bytes
        .saturating_add(summary.diagnostic_export_bytes);
    Ok(summary)
}

fn managed_file_size(data_dir: &Path, path: &Path) -> AppResult<Option<u64>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppError::internal(format!(
            "refusing to follow diagnostic file link '{}'",
            path.display()
        ))),
        Ok(metadata) if metadata.is_file() => {
            ensure_path_within_data_dir(data_dir, path)?;
            Ok(Some(metadata.len()))
        }
        Ok(_) => Err(AppError::internal(format!(
            "diagnostic file path is not a regular file '{}'",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::internal(format!(
            "failed to inspect diagnostic file '{}': {error}",
            path.display()
        ))),
    }
}

fn ensure_path_within_data_dir(data_dir: &Path, path: &Path) -> AppResult<()> {
    let canonical_data_dir = fs::canonicalize(data_dir).map_err(|error| {
        AppError::internal(format!(
            "failed to resolve application data directory '{}': {error}",
            data_dir.display()
        ))
    })?;
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        AppError::internal(format!(
            "failed to resolve diagnostic file '{}': {error}",
            path.display()
        ))
    })?;
    if !canonical_path.starts_with(&canonical_data_dir) {
        return Err(AppError::internal(format!(
            "refusing to access diagnostic path outside its managed directory '{}'",
            path.display()
        )));
    }
    Ok(())
}

fn truncate_file(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map(|_| ())
        .map_err(|error| {
            AppError::internal(format!(
                "failed to clear diagnostic file '{}': {error}",
                path.display()
            ))
        })
}

fn generated_export_directories(root: &Path) -> AppResult<Vec<PathBuf>> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(AppError::internal(format!(
                "refusing to follow diagnostics directory link '{}'",
                root.display()
            )));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(AppError::internal(format!(
                "diagnostics path is not a directory '{}'",
                root.display()
            )));
        }
        Ok(_) => {
            let data_dir = root.parent().ok_or_else(|| {
                AppError::internal(format!(
                    "diagnostics directory has no application data parent '{}'",
                    root.display()
                ))
            })?;
            ensure_path_within_data_dir(data_dir, root)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(AppError::internal(format!(
                "failed to inspect diagnostics directory '{}': {error}",
                root.display()
            )));
        }
    }

    let mut directories = Vec::new();
    let entries = fs::read_dir(root).map_err(|error| {
        AppError::internal(format!(
            "failed to inspect diagnostic exports '{}': {error}",
            root.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::internal(format!("failed to inspect diagnostic export: {error}"))
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_generated_export_name(name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            AppError::internal(format!("failed to inspect diagnostic export: {error}"))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::internal(format!(
                "refusing to follow diagnostic export link '{}'",
                entry.path().display()
            )));
        }
        if metadata.is_dir() {
            ensure_path_within_data_dir(root, &entry.path())?;
            directories.push(entry.path());
        }
    }
    directories.sort_unstable();
    Ok(directories)
}

fn is_generated_export_name(name: &str) -> bool {
    name.strip_prefix(DIAGNOSTIC_EXPORT_PREFIX)
        .is_some_and(|timestamp| {
            !timestamp.is_empty() && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn directory_size(root: &Path, path: &Path) -> AppResult<u64> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        AppError::internal(format!(
            "failed to inspect diagnostic export '{}': {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(AppError::internal(format!(
            "refusing to follow diagnostic export link '{}'",
            path.display()
        )));
    }
    ensure_path_within_data_dir(root, path)?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut size = 0_u64;
    for entry in fs::read_dir(path).map_err(|error| {
        AppError::internal(format!(
            "failed to read diagnostic export '{}': {error}",
            path.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            AppError::internal(format!("failed to inspect diagnostic export: {error}"))
        })?;
        size = size.saturating_add(directory_size(root, &entry.path())?);
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "znet-diagnostic-storage-{name}-{}-{}",
            std::process::id(),
            crate::services::common::now_unix_ms()
        ))
    }

    #[test]
    fn storage_summary_counts_only_known_logs_and_generated_exports() {
        let root = temp_root("summary");
        let logs = root.join("logs");
        let diagnostics = root.join(DIAGNOSTICS_DIR);
        let generated = diagnostics.join(format!("{DIAGNOSTIC_EXPORT_PREFIX}1"));
        let unrelated = diagnostics.join("keep-me");
        let prefix_only = diagnostics.join(DIAGNOSTIC_EXPORT_PREFIX);
        let misleading = diagnostics.join(format!("{DIAGNOSTIC_EXPORT_PREFIX}manual"));
        fs::create_dir_all(&generated).unwrap();
        fs::create_dir_all(&unrelated).unwrap();
        fs::create_dir_all(&prefix_only).unwrap();
        fs::create_dir_all(&misleading).unwrap();
        fs::create_dir_all(&logs).unwrap();
        fs::write(root.join("logs.jsonl"), b"a").unwrap();
        fs::write(logs.join("gui.log.jsonl"), b"bb").unwrap();
        fs::write(logs.join("debug.log.jsonl"), b"ccc").unwrap();
        let core_log = logs.join("core.log.jsonl");
        fs::write(&core_log, b"dddd").unwrap();
        fs::write(generated.join("manifest.json"), b"12345").unwrap();
        fs::write(unrelated.join("private.txt"), b"not-managed").unwrap();

        let summary = summary_at(&root, &core_log).unwrap();

        assert_eq!(summary.live_log_bytes, 10);
        assert_eq!(summary.live_log_file_count, 4);
        assert_eq!(summary.diagnostic_export_bytes, 5);
        assert_eq!(summary.diagnostic_export_count, 1);
        assert_eq!(summary.total_bytes, 15);
        let managed = generated_export_directories(&diagnostics).unwrap();
        assert_eq!(managed, vec![generated.clone()]);
        for directory in managed {
            fs::remove_dir_all(directory).unwrap();
        }
        assert!(!generated.exists());
        assert!(unrelated.exists());
        assert!(prefix_only.exists());
        assert!(misleading.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn truncate_file_clears_content_without_removing_the_file() {
        let root = temp_root("truncate");
        let path = root.join("core.log.jsonl");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, b"sensitive-debug-data").unwrap();

        truncate_file(&path).unwrap();

        assert!(path.is_file());
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_file_size_rejects_non_file_targets() {
        let root = temp_root("non-file");
        fs::create_dir_all(&root).unwrap();

        let error = managed_file_size(&root, &root).unwrap_err();

        assert!(error.message.contains("not a regular file"));
        assert_eq!(
            managed_file_size(&root, &root.join("missing.log")).unwrap(),
            None
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_file_size_rejects_files_outside_the_data_directory() {
        let root = temp_root("boundary");
        let outside = root.with_extension("outside.log");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, b"do-not-clear").unwrap();

        let error = managed_file_size(&root, &outside).unwrap_err();

        assert!(error.message.contains("outside its managed directory"));
        assert_eq!(fs::read(&outside).unwrap(), b"do-not-clear");
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(root);
    }
}
