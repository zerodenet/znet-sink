use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::models::subscription::{
    SubscriptionProfile, SubscriptionRemovalOutcome, SubscriptionRemovalPreview, SubscriptionUpsert,
};
use crate::services::subscription::{self, SyncAllOutcome};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn subscription_list(state: State<'_, AppState>) -> AppResult<Vec<SubscriptionProfile>> {
    subscription::list(state)
}

#[tauri::command]
pub fn subscription_get(state: State<'_, AppState>, id: String) -> AppResult<SubscriptionProfile> {
    subscription::get(state, id)
}

#[tauri::command]
pub fn subscription_upsert(
    state: State<'_, AppState>,
    input: SubscriptionUpsert,
) -> AppResult<SubscriptionProfile> {
    subscription::upsert(state, input)
}

#[tauri::command]
pub async fn subscription_sync(
    app_handle: AppHandle,
    id: String,
) -> AppResult<SubscriptionProfile> {
    subscription::sync(app_handle, id).await
}

#[tauri::command]
pub async fn subscription_sync_all(app_handle: AppHandle) -> AppResult<SyncAllOutcome> {
    subscription::sync_all(app_handle).await
}

#[tauri::command]
pub fn subscription_remove_preview(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<SubscriptionRemovalPreview> {
    subscription::removal_preview(state, id)
}

#[tauri::command]
pub async fn subscription_remove(
    app_handle: AppHandle,
    id: String,
    remove_associated_config: Option<bool>,
) -> AppResult<SubscriptionRemovalOutcome> {
    subscription::remove(app_handle, id, remove_associated_config.unwrap_or(false)).await
}
