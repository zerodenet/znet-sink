use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tauri::State;

use std::io::Read;

use crate::core::ipc;
use crate::errors::{AppError, AppResult};
use crate::models::{
    app_config::AppCoreConfig,
    core::{CoreEndpoint, CoreIpcOptions},
    core_config::{CoreConfigExportResult, CoreConfigSnapshot, CoreDownloadResult, CoreKernelInfo},
};
use super::data_dir;
use crate::services::app_config_store;
use crate::services::common::{self, lock, normalize_optional};
use crate::state::app_state::AppState;

const EXPORTED_CORE_CONFIG_FILE: &str = "zero-active-config.json";
const DEFAULT_CORE_DOWNLOAD_URL: &str = "https://github.com/zerodenet/zero/releases/latest";

pub fn snapshot(state: State<'_, AppState>) -> AppResult<CoreConfigSnapshot> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    snapshot_from_config(&config)
}

pub fn inspect(state: State<'_, AppState>) -> AppResult<CoreKernelInfo> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    let has_active_config = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .any(|p| p.active);
    inspect_from_config(&config, has_active_config)
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
    // 仅可执行文件是用户必须配置的——其余（config 文件、工作目录）由系统自动管理
    if executable_path.is_none() {
        warnings.push("core executable path is not configured".to_string());
    } else if !executable_exists {
        warnings.push("core executable does not exist".to_string());
    }
    // config_path / working_dir 由 export_active() / resolve_working_dir() 自动生成，
    // 不作为用户可见的警告。自检中的 check_active_proxy_config 独立守卫"无活跃配置"场景。

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

pub fn inspect_from_config(config: &AppCoreConfig, has_active_config: bool) -> AppResult<CoreKernelInfo> {
    let executable_path = resolve_executable_path(config);
    let executable_exists = executable_path.as_ref().is_some_and(|path| path.is_file());
    let metadata = executable_path
        .as_ref()
        .and_then(|path| fs::metadata(path).ok());
    let file_name = executable_path.as_ref().and_then(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_string())
    });
    let size_bytes = metadata.as_ref().map(|meta| meta.len());
    let modified_at_unix_ms = metadata
        .as_ref()
        .and_then(|meta| meta.modified().ok())
        .and_then(system_time_to_unix_ms);

    let mut warnings = Vec::new();
    if executable_path.is_none() {
        warnings.push("core executable path is not configured".to_string());
    } else if !executable_exists {
        warnings.push("core executable does not exist".to_string());
    }

    Ok(CoreKernelInfo {
        kernel: config.kernel.clone(),
        executable_path: executable_path.as_deref().map(path_to_string),
        executable_exists,
        file_name,
        size_bytes,
        modified_at_unix_ms,
        recommended_install_dir: recommended_install_dir(),
        download_url: config
            .download_url
            .clone()
            .filter(|url| !url.trim().is_empty())
            .or_else(|| Some(DEFAULT_CORE_DOWNLOAD_URL.to_string())),
        has_active_config,
        warnings,
    })
}

pub fn resolve_executable_path(config: &AppCoreConfig) -> Option<PathBuf> {
    normalize_optional(config.executable_path.clone()).map(PathBuf::from)
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
    Ok(data_dir()?.join(EXPORTED_CORE_CONFIG_FILE))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn system_time_to_unix_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn recommended_install_dir() -> Option<String> {
    // 使用 app data dir + /core，与 kernel_manager::resolve_install_dir 默认路径一致
    data_dir()
        .ok()
        .map(|dir| dir.join("core"))
        .map(|path| path_to_string(&path))
}

pub fn download_latest(install_dir: Option<String>) -> AppResult<CoreDownloadResult> {
    let dir = match install_dir {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d.trim()),
        _ => data_dir()?.join("core"),
    };
    fs::create_dir_all(&dir).map_err(|e| AppError::internal(
        format!("failed to create install dir: {e}"),
    ))?;

    let client = reqwest::blocking::Client::builder()
        .user_agent("znet-sink")
        .build()
        .map_err(|e| AppError::internal(format!("failed to create http client: {e}")))?;

    // Fetch latest release info
    let mut resp = client
        .get("https://api.github.com/repos/zerodenet/zero/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| AppError::internal(format!("failed to fetch release info: {e}")))?;

    let mut body = String::new();
    resp.read_to_string(&mut body)
        .map_err(|e| AppError::internal(format!("failed to read response: {e}")))?;

    let release: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| AppError::internal(format!("failed to parse release info: {e}")))?;

    let version = release["tag_name"].as_str().map(|s: &str| s.to_string());
    let assets = release["assets"].as_array().ok_or_else(|| {
        AppError::internal("no assets found in latest release")
    })?;

    // Determine platform asset name
    let asset_name = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "zero-darwin-aarch64.tar.gz"
    } else if cfg!(target_os = "macos") {
        "zero-darwin-x86_64.tar.gz"
    } else if cfg!(target_os = "linux") {
        "zero-linux-x86_64.tar.gz"
    } else if cfg!(target_os = "windows") {
        "zero-windows-x86_64.zip"
    } else {
        return Err(AppError::internal("unsupported platform"));
    };

    let asset = assets.iter().find(|a| {
        a["name"].as_str().map(|n| n == asset_name).unwrap_or(false)
    }).ok_or_else(|| {
        AppError::internal(format!("asset not found for platform: {asset_name}"))
    })?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| AppError::internal("no download url in asset"))?;

    // Download
    let ext = if asset_name.ends_with(".tar.gz") { "tar.gz" } else { "zip" };
    let temp_file = dir.join(format!("zero-download.{}", ext));

    let mut response = client
        .get(download_url)
        .send()
        .map_err(|e| AppError::internal(format!("failed to download: {e}")))?;

    let mut bytes = Vec::new();
    response.read_to_end(&mut bytes)
        .map_err(|e| AppError::internal(format!("failed to read download: {e}")))?;

    fs::write(&temp_file, &bytes)
        .map_err(|e| AppError::internal(format!("failed to write download: {e}")))?;

    // Extract
    if ext == "tar.gz" {
        let status = common::background_command("tar")
            .args(["-xzf", &path_to_string(&temp_file), "-C", &path_to_string(&dir)])
            .status()
            .map_err(|e| AppError::internal(format!("failed to extract: {e}")))?;
        if !status.success() {
            let _ = fs::remove_file(&temp_file);
            return Err(AppError::internal("failed to extract archive"));
        }
    } else {
        let status = common::background_command("powershell")
            .args([
                "-NoProfile", "-Command",
                &format!("Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    path_to_string(&temp_file), path_to_string(&dir)),
            ])
            .status()
            .map_err(|e| AppError::internal(format!("failed to extract: {e}")))?;
        if !status.success() {
            let _ = fs::remove_file(&temp_file);
            return Err(AppError::internal("failed to extract archive"));
        }
    }

    // Clean up temp file
    let _ = fs::remove_file(&temp_file);

    let executable_path = dir.join(if cfg!(windows) { "zero.exe" } else { "zero" });
    if !executable_path.is_file() {
        return Err(AppError::internal(format!(
            "extracted but could not find binary at: {}",
            executable_path.display()
        )));
    }

    // Make executable on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&executable_path)
            .map_err(|e| AppError::internal(format!("failed to read permissions: {e}")))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&executable_path, perms)
            .map_err(|e| AppError::internal(format!("failed to set executable permissions: {e}")))?;
    }

    let message = format!("zero {} installed to {}", version.as_deref().unwrap_or("?"), path_to_string(&dir));

    Ok(CoreDownloadResult {
        success: true,
        executable_path: path_to_string(&executable_path),
        version,
        message,
    })
}
