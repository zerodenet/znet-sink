use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{atomic_file, data_dir};
use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppConfig;

const CONFIG_FILE_NAME: &str = "app-config.json";
static STORE_LOCK: Mutex<()> = Mutex::new(());

pub fn default_config_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join(CONFIG_FILE_NAME))
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".bak");
    PathBuf::from(name)
}

fn read(path: &Path) -> AppResult<AppConfig> {
    let content = fs::read(path).map_err(|error| io_error(path, "read", error))?;
    serde_json::from_slice(&content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to parse app config: {error}"),
        details: Some(serde_json::json!({ "path": path })),
    })
}

pub fn load_or_default(path: &Path) -> AppResult<AppConfig> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| AppError::internal("app config store lock poisoned"))?;
    let backup = backup_path(path);
    if !path.exists() && !backup.exists() {
        return Ok(AppConfig::default());
    }
    match read(path) {
        Ok(config) => Ok(config),
        Err(original) => {
            let config = read(&backup).map_err(|_| original)?;
            if path.exists() {
                let bytes = fs::read(path).map_err(|error| io_error(path, "preserve", error))?;
                let mut preserved = tempfile::Builder::new()
                    .prefix("app-config-corrupt-")
                    .suffix(".json")
                    .tempfile_in(
                        path.parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or(Path::new(".")),
                    )
                    .map_err(|error| io_error(path, "preserve", error))?;
                use std::io::Write;
                preserved
                    .write_all(&bytes)
                    .map_err(|error| io_error(path, "preserve", error))?;
                preserved
                    .as_file()
                    .sync_all()
                    .map_err(|error| io_error(path, "preserve", error))?;
                preserved
                    .keep()
                    .map_err(|error| io_error(path, "preserve", error.error))?;
            }
            publish(path, &config)?;
            super::file_logger::line("app config: recovered last valid backup");
            Ok(config)
        }
    }
}

pub fn save(path: &Path, config: &AppConfig) -> AppResult<()> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| AppError::internal("app config store lock poisoned"))?;
    let backup = backup_path(path);
    // Only validated bytes may replace the recovery copy. Never rotate a
    // corrupt primary over the last working configuration.
    if path.exists() {
        let previous = read(path)?;
        publish(&backup, &previous)?;
    } else if !backup.exists() {
        publish(&backup, config)?;
    }
    publish(path, config)
}

fn publish(path: &Path, config: &AppConfig) -> AppResult<()> {
    let content = serde_json::to_vec_pretty(config)
        .map_err(|error| AppError::internal(format!("failed to serialize app config: {error}")))?;
    atomic_file::write(path, &content).map_err(|error| io_error(path, "write", error))
}

fn io_error(path: &Path, operation: &str, error: std::io::Error) -> AppError {
    AppError {
        code: "io_error",
        message: format!("failed to {operation} app config: {error}"),
        details: Some(serde_json::json!({ "path": path })),
    }
}

#[cfg(test)]
#[path = "app_config_store_tests.rs"]
mod tests;
