use std::fs;
use std::path::{Path, PathBuf};

use tauri::State;

use crate::core::ipc;
use crate::errors::{AppError, AppResult};
use crate::models::{
    app_config::AppCoreConfig,
    core::{CoreEndpoint, CoreIpcOptions},
    core_config::{CoreConfigExportResult, CoreConfigSnapshot},
};
use crate::services::app_config_store;
use crate::services::common::{lock, normalize_optional};
use crate::state::app_state::AppState;

const EXPORTED_CORE_CONFIG_FILE: &str = "zero-active-config.json";

pub fn snapshot(state: State<'_, AppState>) -> AppResult<CoreConfigSnapshot> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    snapshot_from_config(&config)
}

pub fn export_active(state: State<'_, AppState>) -> AppResult<CoreConfigExportResult> {
    let active = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .cloned()
        .ok_or_else(|| AppError::invalid_argument("no active proxy config"))?;
    let content = active.content.as_ref().ok_or_else(|| {
        AppError::invalid_argument("active proxy config does not contain JSON content")
    })?;
    if !content.is_object() {
        return Err(AppError::invalid_argument(
            "active proxy config content must be a JSON object",
        ));
    }

    let path = default_export_path()?;
    write_core_config(&path, content)?;

    let snapshot = {
        let mut app_config = lock(state.app_config(), "app_config")?;
        app_config.core.config_path = Some(path_to_string(&path));
        app_config_store::save(&app_config_store::default_config_path()?, &app_config)?;
        snapshot_from_config(&app_config.core)?
    };

    Ok(CoreConfigExportResult {
        proxy_config_id: active.id,
        path: path_to_string(&path),
        app_config: snapshot,
    })
}

pub fn ipc_options_from_app_config(config: &AppCoreConfig) -> CoreIpcOptions {
    CoreIpcOptions {
        socket: resolve_socket(config).map(|path| path_to_string(&path)),
        timeout_ms: None,
    }
}

pub fn snapshot_from_config(config: &AppCoreConfig) -> AppResult<CoreConfigSnapshot> {
    let executable_path = resolve_executable_path(config);
    let executable_exists = executable_path.as_ref().is_some_and(|path| path.is_file());
    let working_dir = resolve_working_dir(config, executable_path.as_deref());
    let config_path = normalize_optional(config.config_path.clone()).map(PathBuf::from);
    let socket = resolve_socket(config);
    let endpoint = endpoint_from_socket(socket.as_deref())?;
    let launch_args = launch_args(config_path.as_deref(), socket.as_deref());

    let mut warnings = Vec::new();
    if executable_path.is_none() {
        warnings.push("core executable path is not configured".to_string());
    } else if !executable_exists {
        warnings.push("core executable does not exist".to_string());
    }
    if config_path.is_none() {
        warnings.push("core config file is not configured".to_string());
    } else if !config_path.as_deref().unwrap().is_file() {
        warnings.push("core config file does not exist".to_string());
    }
    if let Some(path) = working_dir.as_deref() {
        if !path.is_dir() {
            warnings.push("core working directory does not exist".to_string());
        }
    }

    Ok(CoreConfigSnapshot {
        kernel: config.kernel.clone(),
        auto_connect: config.auto_connect,
        auto_start: config.auto_start,
        executable_path: executable_path.as_deref().map(path_to_string),
        executable_exists,
        config_path: config_path.as_deref().map(path_to_string),
        config_exists: config_path.as_deref().map(Path::is_file),
        working_dir: working_dir.as_deref().map(path_to_string),
        working_dir_exists: working_dir.as_deref().map(Path::is_dir),
        socket: socket.as_deref().map(path_to_string),
        endpoint,
        launch_args,
        warnings,
    })
}

pub fn resolve_executable_path(config: &AppCoreConfig) -> Option<PathBuf> {
    normalize_optional(config.executable_path.clone())
        .map(PathBuf::from)
        .or_else(|| default_embedded_executable_path(&config.kernel))
}

pub fn resolve_socket(config: &AppCoreConfig) -> Option<PathBuf> {
    if let Some(socket) = normalize_optional(config.socket.clone()) {
        return Some(PathBuf::from(socket));
    }

    default_socket_path(config)
}

pub fn endpoint_from_socket(socket: Option<&Path>) -> AppResult<CoreEndpoint> {
    match socket {
        Some(socket) => Ok(CoreEndpoint {
            transport: ipc::transport_name(),
            path: path_to_string(socket),
        }),
        None => ipc::default_endpoint(),
    }
}

fn resolve_working_dir(config: &AppCoreConfig, executable_path: Option<&Path>) -> Option<PathBuf> {
    normalize_optional(config.working_dir.clone())
        .map(PathBuf::from)
        .or_else(|| {
            executable_path
                .and_then(Path::parent)
                .map(Path::to_path_buf)
        })
}

fn default_embedded_executable_path(kernel: &str) -> Option<PathBuf> {
    if !kernel.eq_ignore_ascii_case("zero") {
        return None;
    }

    let name = if cfg!(windows) { "zero.exe" } else { "zero" };
    Some(workspace_root().join("build").join("core").join(name))
}

fn default_socket_path(config: &AppCoreConfig) -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }

    resolve_executable_path(config)
        .and_then(|path| path.parent().map(|parent| parent.join("zero-control.sock")))
}

fn launch_args(config_path: Option<&Path>, socket: Option<&Path>) -> Vec<String> {
    let mut args = vec!["run".to_string()];
    if let Some(socket) = socket {
        args.push("--control-socket".to_string());
        args.push(path_to_string(socket));
    }
    if let Some(config_path) = config_path {
        args.push(path_to_string(config_path));
    }
    args
}

pub fn write_core_config(path: &Path, content: &serde_json::Value) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError {
            code: "io_error",
            message: format!("failed to create core config directory: {error}"),
            details: Some(serde_json::json!({ "path": parent.display().to_string() })),
        })?;
    }

    let content = serde_json::to_string_pretty(content).map_err(|error| AppError {
        code: "internal",
        message: format!("failed to serialize core config: {error}"),
        details: None,
    })?;
    fs::write(path, content).map_err(|error| AppError {
        code: "io_error",
        message: format!("failed to write core config: {error}"),
        details: Some(serde_json::json!({ "path": path.display().to_string() })),
    })
}

fn default_export_path() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join(EXPORTED_CORE_CONFIG_FILE))
}

fn app_data_dir() -> AppResult<PathBuf> {
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

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
