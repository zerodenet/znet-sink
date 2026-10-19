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
/// Compatibility hook for callers introduced before UI mode became layout-only.
/// Desktop and mobile share this behavior. Real host, feature, permission and
/// resource admission remain the responsibility of each calling use case.
pub fn require_pro_mode(_state: &AppState, _feature: &'static str) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn desktop_and_mobile_modes_are_layout_only() {
        for mode in ["lite", "pro"] {
            let mut config = crate::models::app_config::AppConfig::default();
            config.ui.ui_mode = mode.into();
            let state = crate::state::app_state::AppState::new(config);
            assert!(super::require_pro_mode(&state, "network_endpoints").is_ok());
            assert!(super::require_pro_mode(&state, "config_workspace").is_ok());
        }
    }

    #[test]
    fn layout_admission_does_not_read_or_mutate_preferences() {
        let state = crate::state::app_state::AppState::default();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.app_config().lock().unwrap();
            panic!("poison only this test's preferences");
        }));
        assert!(state.app_config().is_poisoned());
        assert!(super::require_pro_mode(&state, "config_workspace").is_ok());
    }

    #[test]
    fn layout_admission_does_not_grant_host_or_capability_authority() {
        use std::collections::BTreeSet;
        use znet_client_core::capability::{Error, Permission};
        for mode in ["lite", "pro"] {
            let mut config = crate::models::app_config::AppConfig::default();
            config.ui.ui_mode = mode.into();
            let state = crate::state::app_state::AppState::new(config);
            super::require_pro_mode(&state, "config_workspace").unwrap();
            assert!(crate::services::native_documents::require_host("plugin-page").is_err());
            let requested = BTreeSet::from([Permission::new("system.documents", "export")]);
            assert!(matches!(
                state.capabilities().admit(
                    "test.no-system-grant".into(),
                    requested.clone(),
                    requested,
                    &BTreeSet::new(),
                ),
                Err(Error::AdmissionDenied)
            ));
        }
    }
}
