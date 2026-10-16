use crate::{
    configuration::profile_editor,
    errors::{AppError, AppResult},
    models::profile_editor::{DraftInput, SaveInput, Saved, Snapshot, Validated},
    services::{interaction_mode, native_documents},
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, Window};

fn host(state: &AppState, label: &str) -> AppResult<()> {
    native_documents::require_host(label)?;
    interaction_mode::require_pro_mode(state, "profile_editor")
}
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    label: String,
    run: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        host(state.inner(), &label)?;
        run(state.inner())
    })
    .await
    .map_err(|_| AppError::internal("普通配置编辑任务未完成；草稿保留，请核对本地状态"))?
}

#[tauri::command]
pub async fn proxy_config_editor_snapshot(
    app_handle: AppHandle,
    window: Window,
    id: Option<String>,
) -> AppResult<Snapshot> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        profile_editor::snapshot(state, id.as_deref())
    })
    .await
}
#[tauri::command]
pub async fn proxy_config_editor_validate(
    app_handle: AppHandle,
    window: Window,
    input: DraftInput,
) -> AppResult<Validated> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        profile_editor::validate(state, input)
    })
    .await
}
#[tauri::command]
pub async fn proxy_config_editor_save(
    app_handle: AppHandle,
    window: Window,
    input: SaveInput,
) -> AppResult<Saved> {
    let state = app_handle.state::<AppState>();
    host(state.inner(), window.label())?;
    // Only the shared operation coordinator is held across the blocking task;
    // no typed-data/file/core mutex is held over await.
    let _operation = state.proxy_config_operation().lock().await;
    blocking(app_handle.clone(), window.label().into(), move |state| {
        profile_editor::save(state, input)
    })
    .await
}
#[tauri::command]
pub async fn proxy_config_editor_read_file(
    app_handle: AppHandle,
    window: Window,
    path: String,
) -> AppResult<String> {
    host(app_handle.state::<AppState>().inner(), window.label())?;
    blocking(app_handle, window.label().into(), move |state| {
        profile_editor::read_file(
            state.capabilities(),
            path,
            cfg!(any(target_os = "android", target_os = "ios")),
        )
    })
    .await
}
