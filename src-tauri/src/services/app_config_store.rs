use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{
    atomic_file, data_dir,
    file_history::{self, Domain, HistoryToken, WriteReceipt},
};
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
    let mut config: AppConfig = serde_json::from_slice(&content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to parse app config: {error}"),
        details: Some(serde_json::json!({ "path": path })),
    })?;
    super::bypass::normalize(&mut config)?;
    Ok(config)
}

pub fn load_or_default(path: &Path) -> AppResult<AppConfig> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| AppError::internal("app config store lock poisoned"))?;
    let mut history = file_history::acquire(path, Domain::AppConfig)?;
    let backup = backup_path(path);
    if !path.exists() && !backup.exists() {
        let mut config = AppConfig::default();
        super::bypass::normalize(&mut config)?;
        return Ok(config);
    }
    match read(path) {
        // An unresolved write may still expose its actual valid local value for
        // display. It never provides a clean history token or write permission.
        Ok(config) => Ok(config),
        Err(original) => {
            history.token()?;
            let config = read(&backup).map_err(|_| original)?;
            let content = serde_json::to_vec_pretty(&config)
                .map_err(|_| AppError::internal("app config encoding failed"))?;
            history.mutate(&content, None, None, || {
                if path.exists() {
                    let bytes =
                        fs::read(path).map_err(|error| io_error(path, "preserve", error))?;
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
                        .and_then(|_| preserved.as_file().sync_all())
                        .map_err(|error| io_error(path, "preserve", error))?;
                    preserved
                        .keep()
                        .map_err(|error| io_error(path, "preserve", error.error))?;
                }
                atomic_file::write(path, &content).map_err(|error| io_error(path, "write", error))
            })?;
            super::file_logger::line("app config: recovered last valid backup");
            Ok(config)
        }
    }
}
pub fn save(path: &Path, config: &AppConfig) -> AppResult<()> {
    save_impl(path, config, None).map(|_| ())
}
pub(crate) fn save_owned(
    path: &Path,
    config: &AppConfig,
    owner_operation: &str,
) -> AppResult<WriteReceipt> {
    save_impl(path, config, Some(owner_operation))
}
fn save_impl(path: &Path, config: &AppConfig, owner: Option<&str>) -> AppResult<WriteReceipt> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| AppError::internal("app config store lock poisoned"))?;
    let mut history = file_history::acquire(path, Domain::AppConfig)?;
    save_guarded(&mut history, config, owner)
}
/// Lease-aware writes recheck after both store and OS file-history locks.
pub(crate) fn save_authorized(
    path: &Path,
    config: &AppConfig,
    authorize: &dyn Fn() -> AppResult<()>,
) -> AppResult<()> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| AppError::internal("app config store lock poisoned"))?;
    let mut history = file_history::acquire(path, Domain::AppConfig)?;
    authorize()?;
    save_guarded_authorized(&mut history, config, None, Some(authorize)).map(|_| ())
}

/// Owning callers hold AppState's typed CAS guard before the OS history guard.
/// This deliberately does not acquire STORE_LOCK or the same OS lock again.
pub(crate) fn save_owned_locked(
    history: &mut file_history::Guard,
    config: &AppConfig,
    owner: &str,
) -> AppResult<WriteReceipt> {
    if history.domain() != Domain::AppConfig {
        return Err(AppError::invalid_argument("incorrect file history domain"));
    }
    save_guarded(history, config, Some(owner))
}
fn save_guarded(
    history: &mut file_history::Guard,
    config: &AppConfig,
    owner: Option<&str>,
) -> AppResult<WriteReceipt> {
    save_guarded_authorized(history, config, owner, None)
}
fn save_guarded_authorized(
    history: &mut file_history::Guard,
    config: &AppConfig,
    owner: Option<&str>,
    authorize: Option<&dyn Fn() -> AppResult<()>>,
) -> AppResult<WriteReceipt> {
    if let Some(authorize) = authorize {
        authorize()?;
    }
    history.token()?;
    let path = history.path().to_path_buf();
    let path = path.as_path();
    let backup = backup_path(path);
    let content = serde_json::to_vec_pretty(config)
        .map_err(|_| AppError::internal("app config encoding failed"))?;
    let backup_content = if path.exists() {
        Some(
            serde_json::to_vec_pretty(&read(path)?)
                .map_err(|_| AppError::internal("app config encoding failed"))?,
        )
    } else if !backup.exists() {
        Some(content.clone())
    } else {
        None
    };
    let (_, receipt) = history.mutate(&content, backup_content.as_deref(), owner, || {
        if let Some(bytes) = &backup_content {
            if let Some(authorize) = authorize {
                atomic_file::write_authorized(&backup, bytes, authorize)?;
            } else {
                atomic_file::write(&backup, bytes)
                    .map_err(|error| io_error(&backup, "write", error))?;
            }
        }
        if let Some(authorize) = authorize {
            atomic_file::write_authorized(path, &content, authorize)
        } else {
            atomic_file::write(path, &content).map_err(|error| io_error(path, "write", error))
        }
    })?;
    Ok(receipt)
}
pub(crate) fn current_history(path: &Path) -> AppResult<HistoryToken> {
    file_history::acquire(path, Domain::AppConfig)?.token()
}
/// Caller already holds the stable OS guard, so this never reacquires it.
pub(crate) fn read_current_locked(guard: &file_history::Guard) -> AppResult<AppConfig> {
    if guard.domain() != Domain::AppConfig {
        return Err(AppError::invalid_argument("incorrect file history domain"));
    }
    if !guard.path().exists() {
        let mut config = AppConfig::default();
        super::bypass::normalize(&mut config)?;
        return Ok(config);
    }
    read(guard.path())
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
