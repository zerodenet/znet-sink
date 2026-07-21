use tauri::{AppHandle, Manager, State};

use crate::errors::AppResult;
use crate::models::rule_set::{
    CommonRuleBindingInput, CommonRuleInjectionStatus, RuleSetKernelPayload, RuleSetProfile,
    RuleSetSyncAllOutcome, RuleSetUpsert,
};
use crate::services::{interaction_mode, rule_overlay, rule_set};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn rule_set_list(state: State<'_, AppState>) -> AppResult<Vec<RuleSetProfile>> {
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_set::list(state)
}

#[tauri::command]
pub fn rule_set_get(state: State<'_, AppState>, id: String) -> AppResult<RuleSetProfile> {
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_set::get(state, id)
}

#[tauri::command]
pub async fn rule_set_upsert(
    app_handle: AppHandle,
    input: RuleSetUpsert,
) -> AppResult<RuleSetProfile> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    let profile = rule_set::upsert(state, input).await?;
    rule_overlay::reconcile_after_rule_change(app_handle).await?;
    Ok(profile)
}

#[tauri::command]
pub async fn rule_set_remove(app_handle: AppHandle, id: String) -> AppResult<()> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_set::remove(state, id)?;
    rule_overlay::reconcile_after_rule_change(app_handle).await
}

#[tauri::command]
pub async fn rule_set_update(app_handle: AppHandle, id: String) -> AppResult<RuleSetProfile> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    let profile = rule_set::update(state, id).await?;
    rule_overlay::reconcile_after_rule_change(app_handle).await?;
    Ok(profile)
}

#[tauri::command]
pub async fn rule_set_update_all(app_handle: AppHandle) -> AppResult<RuleSetSyncAllOutcome> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    let outcome = rule_set::update_all(state).await?;
    rule_overlay::reconcile_after_rule_change(app_handle).await?;
    Ok(outcome)
}

#[tauri::command]
pub fn rule_set_common_status(state: State<'_, AppState>) -> AppResult<CommonRuleInjectionStatus> {
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_overlay::status(state.inner())
}

#[tauri::command]
pub async fn rule_set_set_common_enabled(
    app_handle: AppHandle,
    enabled: bool,
) -> AppResult<CommonRuleInjectionStatus> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_overlay::set_enabled(app_handle, enabled).await
}

#[tauri::command]
pub async fn rule_set_set_common_binding(
    app_handle: AppHandle,
    input: CommonRuleBindingInput,
) -> AppResult<RuleSetProfile> {
    let state = app_handle.state::<AppState>();
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_overlay::set_binding(app_handle, input).await
}

#[tauri::command]
pub fn rule_set_kernel_payloads(
    state: State<'_, AppState>,
) -> AppResult<Vec<RuleSetKernelPayload>> {
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_set::kernel_payloads(state)
}
