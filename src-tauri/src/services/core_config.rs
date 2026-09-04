use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tauri::State;

use super::data_dir;
use crate::errors::{AppError, AppResult};
use crate::kernel::transport;
use crate::models::{
    app_config::AppCoreConfig,
    core::{CoreEndpoint, CoreIpcOptions},
    core_config::{CoreConfigExportResult, CoreConfigSnapshot, CoreKernelInfo},
};
use crate::services::common::{lock, normalize_optional};
use crate::services::{app_config_store, rule_overlay};
use crate::state::app_state::AppState;

const EXPORTED_CORE_CONFIG_FILE: &str = "zero-active-config.json";
const MANAGED_CORE_LOG_FILE: &str = "core.log.jsonl";

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
    // Strip GUI-only fields the core engine doesn't understand, then inject
    // a managed file sink so kernel runtime logs survive GUI restarts/crashes.
    let effective = rule_overlay::compose_effective_config(state.inner(), content)?;
    let mut export_content = strip_gui_only_fields(&effective);
    inject_managed_core_log(&mut export_content)?;
    write_core_config(&path, &export_content)?;

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
    let socket = resolve_socket_for_runtime(config, executable_path.as_deref());
    let endpoint = endpoint_from_socket(socket.as_deref())?;
    let launch_socket = resolve_launch_socket(config, executable_path.as_deref());
    let launch_args = launch_args(config_path.as_deref(), launch_socket.as_deref());

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

pub fn inspect_from_config(
    config: &AppCoreConfig,
    has_active_config: bool,
) -> AppResult<CoreKernelInfo> {
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
        has_active_config,
        warnings,
    })
}

pub fn resolve_executable_path(config: &AppCoreConfig) -> Option<PathBuf> {
    normalize_optional(config.executable_path.clone())
        .map(PathBuf::from)
        .or_else(discover_executable_path)
}

/// Locate a Zero binary supplied by the application or an adjacent development
/// checkout. An explicitly configured path always wins, including when it is
/// currently missing, so a user choice is never silently replaced.
fn discover_executable_path() -> Option<PathBuf> {
    let executable_name = if cfg!(windows) { "zero.exe" } else { "zero" };
    let mut candidates = Vec::new();

    // Kernel manager installs releases here. This also makes a previously
    // installed kernel usable after app-config.json is recreated.
    if let Ok(dir) = data_dir() {
        candidates.push(dir.join("core").join(executable_name));
    }

    // Packaged/sidecar builds place the kernel beside the GUI executable.
    if let Ok(gui_executable) = std::env::current_exe() {
        if let Some(dir) = gui_executable.parent() {
            candidates.push(dir.join(executable_name));
        }
    }

    // Developer layout: rust/gui/src-tauri and rust/zero are sibling projects.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(rust_dir) = manifest_dir.parent().and_then(Path::parent) {
        let zero_target = rust_dir.join("zero").join("target");
        if cfg!(debug_assertions) {
            candidates.push(zero_target.join("debug").join(executable_name));
            candidates.push(zero_target.join("release").join(executable_name));
        } else {
            candidates.push(zero_target.join("release").join(executable_name));
            candidates.push(zero_target.join("debug").join(executable_name));
        }
    }

    candidates.into_iter().find(|path| path.is_file())
}

pub fn resolve_socket(config: &AppCoreConfig) -> Option<PathBuf> {
    resolve_socket_for_runtime(config, resolve_executable_path(config).as_deref())
}

fn resolve_socket_for_runtime(
    config: &AppCoreConfig,
    executable_path: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(socket) = normalize_optional(config.socket.clone()) {
        return Some(PathBuf::from(socket));
    }

    default_runtime_socket_path(config, executable_path)
}

pub fn endpoint_from_socket(socket: Option<&Path>) -> AppResult<CoreEndpoint> {
    match socket {
        Some(socket) => Ok(CoreEndpoint {
            transport: transport::transport_name(),
            path: path_to_string(socket),
        }),
        None => transport::default_endpoint("zero"),
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

/// Socket path used when spawning a GUI-managed kernel.
///
/// On Windows, the named pipe (`\\.\pipe\zero-control`) is resolved by
/// `transport::default_endpoint` — no file path needed here.
/// On Unix, defaults to the Zero daemon path: `~/.zero/control.sock`.
fn resolve_launch_socket(
    config: &AppCoreConfig,
    executable_path: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(socket) = normalize_optional(config.socket.clone()) {
        return Some(PathBuf::from(socket));
    }

    default_launch_socket_path(config, executable_path)
}

#[cfg(windows)]
fn default_runtime_socket_path(
    _config: &AppCoreConfig,
    _executable_path: Option<&Path>,
) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
fn default_runtime_socket_path(
    config: &AppCoreConfig,
    executable_path: Option<&Path>,
) -> Option<PathBuf> {
    executable_path
        .map(|path| {
            PathBuf::from(transport::default_socket_path_for_executable(
                Some(path),
                &config.kernel,
            ))
        })
        .or_else(external_default_socket_path)
}

#[cfg(windows)]
fn default_launch_socket_path(
    _config: &AppCoreConfig,
    _executable_path: Option<&Path>,
) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
fn default_launch_socket_path(
    config: &AppCoreConfig,
    executable_path: Option<&Path>,
) -> Option<PathBuf> {
    executable_path.map(|path| {
        PathBuf::from(transport::default_socket_path_for_executable(
            Some(path),
            &config.kernel,
        ))
    })
}

#[cfg(unix)]
fn external_default_socket_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".zero").join("control.sock"))
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

/// Write a minimal temp config so the kernel can start its control plane
/// even without any proxy configuration. The GUI shows "kernel running,
/// no proxy config" instead of treating this as a startup failure.
pub fn write_minimal_temp_config() -> AppResult<PathBuf> {
    let path = data_dir()?.join("zero-temp-stub.json");
    let mut minimal = minimal_temp_config_content();
    inject_managed_core_log(&mut minimal)?;
    write_core_config(&path, &minimal)?;
    Ok(path)
}

fn minimal_temp_config_content() -> serde_json::Value {
    serde_json::json!({
        "inbounds": [],
        "outbounds": [],
        "outbound_groups": [],
        "runtime": {},
        "api": {
            "control": {
                // GUI management uses the dedicated local control socket
                // passed through --control-socket. The HTTP status API is a
                // separate authenticated surface and must stay disabled.
                "enabled": false
            }
        },
        "mode": { "type": "rule" },
        "route": {
            "rules": [],
            "final": { "type": "direct" }
        }
    })
}

pub fn managed_core_log_path() -> AppResult<PathBuf> {
    Ok(data_dir()?.join("logs").join(MANAGED_CORE_LOG_FILE))
}

fn inject_managed_core_log(content: &mut serde_json::Value) -> AppResult<()> {
    let path = managed_core_log_path()?;
    inject_managed_core_log_at_path(content, &path)
}

fn inject_managed_core_log_at_path(
    content: &mut serde_json::Value,
    log_path: &Path,
) -> AppResult<()> {
    let root = content.as_object_mut().ok_or_else(|| {
        AppError::invalid_argument("active proxy config content must be a JSON object")
    })?;
    let runtime = ensure_object_field(root, "runtime")?;
    let log = ensure_object_field(runtime, "log")?;
    let files = ensure_array_field(log, "files")?;
    let log_path = path_to_string(log_path);

    let already_present = files.iter().any(|entry| {
        entry
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|path| path == log_path)
    });
    if !already_present {
        files.push(serde_json::json!({
            "path": log_path,
            "max_bytes": 10 * 1024 * 1024u64,
            "max_files": 5
        }));
    }

    Ok(())
}

fn ensure_object_field<'a>(
    root: &'a mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> AppResult<&'a mut serde_json::Map<String, serde_json::Value>> {
    if !root.contains_key(key) {
        root.insert(
            key.to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
    }
    root.get_mut(key)
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| AppError::invalid_argument(format!("{key} must be an object")))
}

fn ensure_array_field<'a>(
    root: &'a mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> AppResult<&'a mut Vec<serde_json::Value>> {
    if !root.contains_key(key) {
        root.insert(key.to_string(), serde_json::Value::Array(Vec::new()));
    }
    root.get_mut(key)
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| AppError::invalid_argument(format!("{key} must be an array")))
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

/// Convert legacy GUI-only routing state into the Zero config schema accepted by the core.
fn strip_gui_only_fields(content: &serde_json::Value) -> serde_json::Value {
    let mut cleaned = content.clone();
    if let Some(obj) = cleaned.as_object_mut() {
        translate_legacy_route_mode_for_export(obj);
    }
    cleaned
}

fn translate_legacy_route_mode_for_export(root: &mut serde_json::Map<String, serde_json::Value>) {
    let has_top_level_mode = root.get("mode").is_some();
    let Some(route) = root
        .get_mut("route")
        .and_then(|route| route.as_object_mut())
    else {
        return;
    };
    let mode = route.remove("mode");
    let Some(mode) = mode.as_ref() else {
        return;
    };
    let mode_to_promote = (!has_top_level_mode).then(|| mode.clone());

    match route_mode_kind(mode).as_deref() {
        Some("global") => {
            let outbound = route_mode_outbound(mode)
                .or_else(|| route_final_outbound(route))
                .unwrap_or_else(|| "proxy".to_string());
            route.insert(
                "final".to_string(),
                serde_json::json!({ "type": "route", "outbound": outbound }),
            );
            route.insert("rules".to_string(), serde_json::Value::Array(Vec::new()));
            route.remove("rule_sets");
        }
        Some("direct") => {
            route.insert("final".to_string(), serde_json::json!({ "type": "direct" }));
            route.insert("rules".to_string(), serde_json::Value::Array(Vec::new()));
            route.remove("rule_sets");
        }
        Some("rule") | None => {}
        Some(_) => {}
    }
    let _ = route;
    if let Some(mode) = mode_to_promote {
        root.insert("mode".to_string(), mode);
    }
}

fn route_mode_kind(mode: &serde_json::Value) -> Option<String> {
    mode.as_str()
        .or_else(|| mode.get("type").and_then(serde_json::Value::as_str))
        .or_else(|| mode.get("kind").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(|kind| kind.to_ascii_lowercase())
}

fn route_mode_outbound(mode: &serde_json::Value) -> Option<String> {
    mode.get("outbound")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|outbound| !outbound.is_empty())
        .map(ToString::to_string)
}

fn route_final_outbound(route: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    route
        .get("final")
        .and_then(|final_route| final_route.get("outbound"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|outbound| !outbound.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::{inject_managed_core_log_at_path, strip_gui_only_fields};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn exported_config_strips_route_mode_and_maps_global_to_final_route() {
        let content = json!({
            "mode": { "type": "global", "outbound": "legacy-proxy" },
            "route": {
                "mode": { "type": "global", "outbound": "proxy" },
                "rules": [{ "action": { "type": "direct" } }],
                "rule_sets": ["cn"],
                "final": { "type": "direct" }
            }
        });

        let cleaned = strip_gui_only_fields(&content);

        assert_eq!(
            cleaned["mode"],
            json!({ "type": "global", "outbound": "legacy-proxy" })
        );
        assert!(cleaned["route"].get("mode").is_none());
        assert_eq!(
            cleaned["route"]["final"],
            json!({ "type": "route", "outbound": "proxy" })
        );
        assert_eq!(cleaned["route"]["rules"], json!([]));
        assert!(cleaned["route"].get("rule_sets").is_none());
    }

    #[test]
    fn exported_config_strips_route_mode_and_maps_direct_to_final_direct() {
        let content = json!({
            "route": {
                "mode": { "type": "direct" },
                "rules": [{ "action": { "type": "route", "outbound": "proxy" } }],
                "final": { "type": "route", "outbound": "proxy" }
            }
        });

        let cleaned = strip_gui_only_fields(&content);

        assert_eq!(cleaned["mode"], json!({ "type": "direct" }));
        assert!(cleaned["route"].get("mode").is_none());
        assert_eq!(cleaned["route"]["final"], json!({ "type": "direct" }));
        assert_eq!(cleaned["route"]["rules"], json!([]));
    }

    #[test]
    fn exported_config_strips_route_mode_and_preserves_rule_routing() {
        let content = json!({
            "route": {
                "mode": { "type": "rule" },
                "rules": [{ "action": { "type": "direct" } }],
                "final": { "type": "route", "outbound": "proxy" }
            }
        });

        let cleaned = strip_gui_only_fields(&content);

        assert_eq!(cleaned["mode"], json!({ "type": "rule" }));
        assert!(cleaned["route"].get("mode").is_none());
        assert_eq!(
            cleaned["route"]["rules"],
            json!([{ "action": { "type": "direct" } }])
        );
        assert_eq!(
            cleaned["route"]["final"],
            json!({ "type": "route", "outbound": "proxy" })
        );
    }

    #[test]
    fn minimal_temp_config_uses_socket_control_without_http_api() {
        let saved = super::minimal_temp_config_content();

        assert_eq!(saved["inbounds"], json!([]));
        assert_eq!(saved["outbounds"], json!([]));
        assert_eq!(saved["api"]["control"]["enabled"], json!(false));
        assert!(saved["api"]["control"].get("listen").is_none());
        assert_eq!(saved["mode"], json!({ "type": "rule" }));
        assert_eq!(saved["route"]["final"], json!({ "type": "direct" }));
    }

    #[test]
    fn managed_core_log_sink_is_injected_into_exported_runtime() {
        let mut content = json!({
            "route": {
                "rules": []
            }
        });

        inject_managed_core_log_at_path(&mut content, Path::new("C:/tmp/core.log.jsonl")).unwrap();

        assert_eq!(
            content["runtime"]["log"]["files"],
            json!([{
                "path": "C:/tmp/core.log.jsonl",
                "max_bytes": 10485760u64,
                "max_files": 5
            }])
        );
    }

    #[test]
    fn managed_core_log_sink_is_not_duplicated() {
        let mut content = json!({
            "runtime": {
                "log": {
                    "files": [{
                        "path": "C:/tmp/core.log.jsonl",
                        "max_bytes": 10485760u64,
                        "max_files": 5
                    }]
                }
            }
        });

        inject_managed_core_log_at_path(&mut content, Path::new("C:/tmp/core.log.jsonl")).unwrap();

        assert_eq!(
            content["runtime"]["log"]["files"].as_array().unwrap().len(),
            1
        );
    }
}
