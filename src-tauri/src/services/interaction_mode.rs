use crate::errors::{AppError, AppResult};
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

pub fn require_pro_mode(state: &AppState, feature: &'static str) -> AppResult<()> {
    let ui_mode = current_ui_mode(state)?;
    // Desktop Lite is a presentation choice. Native mobile always exposes
    // management; each use case retains its own capability/permission checks.
    if management_mode_allowed(
        &ui_mode,
        cfg!(any(target_os = "android", target_os = "ios")),
    ) {
        return Ok(());
    }

    Err(AppError::mode_restricted(feature, UI_MODE_PRO, ui_mode))
}

fn management_mode_allowed(ui_mode: &str, native_mobile: bool) -> bool {
    native_mobile || is_pro_mode(ui_mode)
}

#[cfg(test)]
mod tests {
    #[test]
    fn mobile_presentation_does_not_change_desktop_mode_authority() {
        assert!(super::management_mode_allowed("lite", true));
        assert!(!super::management_mode_allowed("lite", false));
        assert!(super::management_mode_allowed("pro", false));
    }
}
