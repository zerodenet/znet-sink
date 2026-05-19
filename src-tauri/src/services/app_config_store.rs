use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{AppError, AppResult};
use crate::models::app_config::AppConfig;

const CONFIG_FILE_NAME: &str = "app-config.json";

pub fn default_config_path() -> AppResult<PathBuf> {
    Ok(app_config_dir()?.join(CONFIG_FILE_NAME))
}

pub fn load_or_default(path: &Path) -> AppResult<AppConfig> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let content = fs::read_to_string(path).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to read app config: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })?;

    serde_json::from_str(&content).map_err(|error| AppError {
        code: "invalid_argument",
        message: format!("failed to parse app config: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

pub fn save(path: &Path, config: &AppConfig) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to create app config directory: {error}"),
            details: Some(serde_json::json!({ "path": parent.display().to_string() })),
        })?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|error| AppError {
        code: "internal",
        message: format!("failed to serialize app config: {error}"),
        details: None,
    })?;

    fs::write(path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write app config: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn app_config_dir() -> AppResult<PathBuf> {
    if let Some(path) = std::env::var_os("ZNET_SINK_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    if let Some(app_data) = std::env::var_os("APPDATA") {
        return Ok(PathBuf::from(app_data).join("ZNet Sink"));
    }

    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(config_home).join("znet-sink"));
    }

    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home).join(".config").join("znet-sink"));
    }

    Ok(std::env::current_dir()
        .map_err(|error| AppError::internal(format!("failed to resolve current dir: {error}")))?
        .join(".znet-sink"))
}
