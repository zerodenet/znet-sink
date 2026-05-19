use tauri::State;

use crate::errors::AppResult;
use crate::models::subscription::{SubscriptionProfile, SubscriptionUpsert};
use crate::services::subscription;
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
    state: State<'_, AppState>,
    id: String,
) -> AppResult<SubscriptionProfile> {
    subscription::sync(state, id).await
}

#[tauri::command]
pub fn subscription_remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    subscription::remove(state, id)
}
