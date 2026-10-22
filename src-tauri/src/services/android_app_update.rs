//! Android application installation is separate from kernel/plugin installation.
use crate::{
    errors::{AppError, AppResult},
    models::mobile_release::*,
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_znet_native_io::{AppUpdateRequest, AppUpdateSnapshot, NativeIoExt};

pub(crate) fn read(
    app: &AppHandle,
    action: &str,
    operation_id: Option<String>,
) -> AppResult<AppUpdateSnapshot> {
    run(app, AppUpdateRequest::action(action, operation_id))
}

pub(crate) fn download(
    app: &AppHandle,
    channel: MobileReleaseChannel,
    tag: &str,
    asset_id: &str,
    expected_sha256: &str,
    operation_id: String,
) -> AppResult<AppUpdateSnapshot> {
    if !cfg!(target_os = "android") {
        return Err(AppError::invalid_argument("当前平台没有 Android 安装器"));
    }
    let state = app.state::<AppState>();
    // Re-read authoritative catalog; never trust a URL, checksum, version or path supplied by a page.
    let catalog =
        super::mobile_release::catalog(state.inner(), MobileReleaseSource::Client, channel)?;
    let release = catalog
        .releases
        .iter()
        .find(|r| r.tag_name == tag)
        .ok_or_else(|| AppError::invalid_argument("发行版本已变化，请重新读取目录"))?;
    let asset = release
        .android_assets
        .iter()
        .find(|a| a.asset_id == asset_id && a.sha256 == expected_sha256)
        .ok_or_else(|| AppError::invalid_argument("APK 资产或摘要已变化，请重新审阅"))?;
    let mut request = AppUpdateRequest::action("download", Some(operation_id));
    request.asset_url = Some(asset.url.clone());
    request.asset_name = Some(asset.name.clone());
    request.sha256 = Some(asset.sha256.clone());
    request.size_bytes = Some(asset.size_bytes.clone());
    request.version = Some(release.version.clone());
    run(app, request)
}
fn run(app: &AppHandle, request: AppUpdateRequest) -> AppResult<AppUpdateSnapshot> {
    if !request.valid() {
        return Err(AppError::invalid_argument("安装操作身份或参数无效"));
    }
    app.native_io()
        .app_update(&request)
        .map_err(|_| AppError::internal("Android 更新回执未确认；请核对原操作，不要自动重放"))
}
