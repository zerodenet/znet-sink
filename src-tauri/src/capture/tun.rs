use super::tun_restore;
use crate::errors::AppResult;
use crate::kernel::zero;
use crate::services::{common, core_config};
use crate::state::app_state::AppState;
use std::time::Duration;
const TUN_RESTORE_TIMEOUT: Duration = Duration::from_secs(8);
const TUN_RESTORE_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) async fn app_tun_runtime_enabled(state: &AppState) -> AppResult<bool> {
    let app_config = common::lock(state.app_config(), "app_config")?.clone();
    let options = core_config::ipc_options_from_app_config(&app_config.core);
    zero::runtime::tun_status(Some(options))
        .await
        .map(|status| status.enabled)
}

pub(crate) async fn restore_app_tun_after_core_transition_with_desired(
    state: &AppState,
    desired_enabled: Option<bool>,
) -> AppResult<()> {
    let app_config = common::lock(state.app_config(), "app_config")?.clone();
    let should_enable = desired_enabled.unwrap_or(app_config.tun.enabled == Some(true));
    if !should_enable {
        return Ok(());
    }

    let options = core_config::ipc_options_from_app_config(&app_config.core);
    tun_restore::restore(
        || zero::runtime::tun_status(Some(options.clone())),
        || zero::runtime::enable_tun(app_config.tun.clone(), Some(options.clone())),
        TUN_RESTORE_TIMEOUT,
        TUN_RESTORE_INTERVAL,
    )
    .await
}

pub(crate) async fn restore_app_tun_after_core_transition(state: &AppState) -> AppResult<()> {
    restore_app_tun_after_core_transition_with_desired(state, None).await
}
