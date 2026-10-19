use tauri::{AppHandle, Manager, State, Window};

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
    // Tauri's Release responder retains this future by value. Keep the
    // download/configuration state on the heap before crossing that boundary.
    Box::pin(subscription::sync(app_handle, id)).await
}

#[tauri::command]
pub async fn subscription_sync_all(app_handle: AppHandle) -> AppResult<SyncAllOutcome> {
    Box::pin(subscription::sync_all(app_handle)).await
}

/// Explicit main-host action. Only validator metadata is removed; an unknown
/// configuration application is never resolved by deleting this row.
#[tauri::command]
pub async fn subscription_reset_conditional_cache(
    app_handle: AppHandle,
    window: Window,
    id: String,
    expected_updated_at_unix_ms: String,
) -> AppResult<subscription::ConditionalCacheReset> {
    crate::services::native_documents::require_host(window.label())?;
    let version = expected_updated_at_unix_ms
        .parse::<u64>()
        .map_err(|_| crate::errors::AppError::invalid_argument("来源版本无法确认，请刷新后重试"))?;
    if version.to_string() != expected_updated_at_unix_ms {
        return Err(crate::errors::AppError::invalid_argument(
            "来源版本无法确认，请刷新后重试",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        subscription::reset_conditional_cache(app_handle.state::<AppState>(), id, version)
    })
    .await
    .map_err(|_| {
        crate::errors::AppError::internal("条件记录清理结果未确认，请先核对；不会自动重试")
    })?
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

/// Inspection never writes. A recovery offer requires a durable confirmed
/// receipt; readable current identity alone never clears an uncertain operation.
#[tauri::command]
pub async fn subscription_reconcile_unknown(
    app_handle: AppHandle,
    window: Window,
    id: String,
    expected_updated_at_unix_ms: String,
) -> AppResult<crate::services::subscription_reconciliation::SubscriptionReconciliation> {
    crate::services::native_documents::require_host(window.label())?;
    let version = parse_reconciliation_version(&expected_updated_at_unix_ms)?;
    crate::services::subscription_reconciliation::inspect(
        app_handle.state::<AppState>().inner(),
        id,
        version,
    )
    .await
}

/// Explicit local publication only, bound to the user's inspected recovery
/// record. This command cannot download subscription content or apply runtime
/// configuration again.
#[tauri::command]
pub async fn subscription_recover_confirmed_local(
    app_handle: AppHandle,
    window: Window,
    id: String,
    expected_updated_at_unix_ms: String,
    recovery_id: String,
) -> AppResult<SubscriptionProfile> {
    crate::services::native_documents::require_host(window.label())?;
    let version = parse_reconciliation_version(&expected_updated_at_unix_ms)?;
    crate::services::subscription_reconciliation::recover(
        app_handle.state::<AppState>().inner(),
        id,
        version,
        recovery_id,
    )
    .await
}
fn parse_reconciliation_version(value: &str) -> AppResult<u64> {
    let version = value
        .parse::<u64>()
        .map_err(|_| crate::errors::AppError::invalid_argument("来源版本无法确认，请刷新后重试"))?;
    if version.to_string() != value {
        return Err(crate::errors::AppError::invalid_argument(
            "来源版本无法确认，请刷新后重试",
        ));
    }
    Ok(version)
}
#[cfg(test)]
mod reconciliation_tests {
    use super::*;
    #[test]
    fn reconciliation_is_host_only_and_versions_are_lossless_canonical_decimal() {
        assert!(crate::services::native_documents::require_host("main").is_ok());
        assert!(crate::services::native_documents::require_host("plugin-canary").is_err());
        for value in ["", " 1", "01", "+1", "-1", "1.0", "18446744073709551616"] {
            assert!(parse_reconciliation_version(value).is_err());
        }
        assert_eq!(
            parse_reconciliation_version("18446744073709551615").unwrap(),
            u64::MAX
        );
    }
}
