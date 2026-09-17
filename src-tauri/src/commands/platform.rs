use std::time::Duration;

use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;
use znet_client_core::capability::Permission;

use crate::{
    errors::{AppError, AppResult},
    state::app_state::AppState,
};

fn execute(
    state: &AppState,
    capability: &str,
    scope: &str,
    bytes: usize,
    work: impl FnOnce() -> AppResult<()>,
) -> AppResult<()> {
    let permission = Permission::new(capability, scope);
    let lease = znet_client_capabilities::host::operation(
        state.capabilities(),
        &format!("builtin.platform:{capability}"),
        permission.clone(),
        1,
        bytes.max(1),
        Duration::from_secs(15),
    )
    .map_err(capability_error)?;
    let mut failure = None;
    lease
        .execute(&permission, || {
            work().map(|value| (value, bytes)).map_err(|error| {
                failure = Some(error);
                znet_client_core::capability::Error::Transport
            })
        })
        .and_then(|resource| resource.take(&lease))
        .map_err(|error| failure.unwrap_or_else(|| capability_error(error)))
}

fn capability_error(error: znet_client_core::capability::Error) -> AppError {
    AppError::internal(format!("平台操作被客户端能力策略拒绝：{error}"))
}

#[tauri::command]
pub fn platform_open_url(app: AppHandle, state: State<'_, AppState>, url: String) -> AppResult<()> {
    if !matches!(
        znet_client_capabilities::network::origin(&url),
        Ok(origin) if origin.starts_with("https://") || origin.starts_with("http://")
    ) {
        return Err(AppError::invalid_argument("只能打开有效的 HTTP(S) 地址"));
    }
    execute(
        state.inner(),
        "platform.open-url",
        "external",
        url.len(),
        || {
            app.opener()
                .open_url(url, None::<String>)
                .map_err(|error| AppError::internal(format!("无法打开浏览器：{error}")))
        },
    )
}

#[tauri::command]
pub fn platform_open_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<()> {
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        return Err(AppError::invalid_argument("只能打开绝对路径"));
    }
    let scope = path.to_string_lossy().to_string();
    execute(
        state.inner(),
        "platform.open-path",
        &scope,
        scope.len(),
        || {
            app.opener()
                .open_path(path.to_string_lossy().to_string(), None::<String>)
                .map_err(|error| AppError::internal(format!("无法打开路径：{error}")))
        },
    )
}

#[tauri::command]
pub fn platform_reveal_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<()> {
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        return Err(AppError::invalid_argument("只能定位绝对路径"));
    }
    let scope = path.to_string_lossy().to_string();
    execute(
        state.inner(),
        "platform.reveal-path",
        &scope,
        scope.len(),
        || {
            app.opener()
                .reveal_item_in_dir(path)
                .map_err(|error| AppError::internal(format!("无法在目录中定位文件：{error}")))
        },
    )
}

#[tauri::command]
pub fn platform_clipboard_write(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> AppResult<()> {
    if text.len() > 8 * 1024 * 1024 {
        return Err(AppError::invalid_argument("剪贴板内容超过 8 MiB 限制"));
    }
    execute(
        state.inner(),
        "platform.clipboard-write",
        "text",
        text.len(),
        || {
            app.clipboard()
                .write_text(text)
                .map_err(|error| AppError::internal(format!("系统拒绝了剪贴板写入：{error}")))
        },
    )
}
