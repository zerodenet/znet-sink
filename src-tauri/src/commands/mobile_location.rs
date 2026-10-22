use crate::{
    configuration::mobile_location,
    errors::{AppError, AppResult},
    models::mobile_location::Snapshot,
    services::native_documents,
    state::app_state::AppState,
};
use tauri::{AppHandle, Manager, Window};

#[tauri::command]
pub async fn gui_mobile_location_snapshot(
    app_handle: AppHandle,
    window: Window,
) -> AppResult<Snapshot> {
    native_documents::require_host(window.label())?;
    let label = window.label().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        native_documents::require_host(&label)?;
        mobile_location::snapshot(app_handle.state::<AppState>().inner())
    })
    .await
    .map_err(|_| AppError::internal("当前定位读取未完成，请刷新后重试"))?
}
