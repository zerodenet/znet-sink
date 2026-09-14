#[tauri::command]
pub fn plugins_supported() -> bool {
    cfg!(not(any(target_os = "android", target_os = "ios")))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop {
    use crate::{
        errors::AppResult,
        services::plugins::{Review, Snapshot},
        state::app_state::AppState,
    };
    use tauri::{AppHandle, Manager, State};
    use znet_plugin_sandbox::contract::Request;
    async fn blocking<T: Send + 'static>(
        app: AppHandle,
        work: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
    ) -> AppResult<T> {
        tauri::async_runtime::spawn_blocking(move || work(app.state::<AppState>().inner()))
            .await
            .map_err(|_| crate::errors::AppError::internal("插件操作未能完成"))?
    }
    #[tauri::command]
    pub fn plugins_snapshot(state: State<'_, AppState>) -> Snapshot {
        state.plugins().snapshot(state.capabilities())
    }
    #[tauri::command]
    pub async fn plugins_refresh(app: AppHandle) -> AppResult<Snapshot> {
        blocking(app, |state| state.plugins().refresh(state.capabilities())).await
    }
    #[tauri::command]
    pub async fn plugins_authorize(
        app: AppHandle,
        review: Review,
        grants: Vec<Request>,
    ) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state
                .plugins()
                .authorize(state.capabilities(), review, grants)
        })
        .await
    }
    #[tauri::command]
    pub fn plugins_stop(state: State<'_, AppState>, key: String) -> Snapshot {
        state.plugins().stop(state.capabilities(), &key)
    }
    #[tauri::command]
    pub async fn plugins_run(app: AppHandle, review: Review) -> AppResult<serde_json::Value> {
        blocking(app, move |state| {
            state.plugins().run(state.capabilities(), review)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_install(app: AppHandle, path: String) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state.plugins().install(state.capabilities(), path)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_catalog(
        app: AppHandle,
    ) -> AppResult<Vec<znet_plugin_sandbox::distribution::directory::Registration>> {
        blocking(app, |state| state.plugins().catalog(state.capabilities())).await
    }
    #[tauri::command]
    pub async fn plugins_releases(
        app: AppHandle,
        id: String,
    ) -> AppResult<Vec<znet_plugin_sandbox::distribution::remote::Release>> {
        blocking(app, move |state| {
            state.plugins().releases(state.capabilities(), &id)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_install_release(
        app: AppHandle,
        id: String,
        tag: String,
    ) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state
                .plugins()
                .install_release(state.capabilities(), &id, &tag)
        })
        .await
    }
    #[tauri::command]
    pub async fn plugins_uninstall(app: AppHandle, id: String) -> AppResult<Snapshot> {
        blocking(app, move |state| {
            state.plugins().uninstall(state.capabilities(), &id)
        })
        .await
    }
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use desktop::*;
