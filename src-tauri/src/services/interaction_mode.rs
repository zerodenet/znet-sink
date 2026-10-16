use crate::errors::AppResult;
use crate::services::common::lock;
use crate::state::app_state::AppState;

pub const UI_MODE_LITE: &str = "lite";
pub const UI_MODE_PRO: &str = "pro";

pub fn current_ui_mode(state: &AppState) -> AppResult<String> {
    Ok(lock(state.app_config(), "app_config")?.ui.ui_mode.clone())
}

pub fn is_pro_mode(ui_mode: &str) -> bool {
    ui_mode.eq_ignore_ascii_case(UI_MODE_PRO)
}
