use tauri::State;

use crate::errors::AppResult;
use crate::models::rule_set::{RuleSetProfile, RuleSetUpsert};
use crate::services::{interaction_mode, rule_set};
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
pub fn rule_set_upsert(
    state: State<'_, AppState>,
    input: RuleSetUpsert,
) -> AppResult<RuleSetProfile> {
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_set::upsert(state, input)
}

#[tauri::command]
pub fn rule_set_remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    interaction_mode::require_pro_mode(state.inner(), "ruleSets")?;
    rule_set::remove(state, id)
}
