use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppConfigPatch, AppTunConfigPatch};
use crate::services::common;
use crate::state::app_state::AppState;
use tauri::{AppHandle, State};

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod transaction;
mod validation;

// Compatibility entry point follows the same global settings transaction as the UI.
pub(super) async fn apply(
    handle: AppHandle,
    state: State<'_, AppState>,
    patch: AppTunConfigPatch,
) -> AppResult<AppConfig> {
    if patch.enabled.is_some() {
        return Err(AppError::invalid_argument(
            "use the TUN toggle to change its enabled state",
        ));
    }
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    let (id, source) = crate::configuration::local_edits::context(state.inner())?;
    let candidate = crate::configuration::local_edits::prepare_patch(
        &previous,
        id.as_deref(),
        &source,
        AppConfigPatch {
            tun: Some(patch),
            ..Default::default()
        },
    )?;
    let effective =
        crate::configuration::local_edits::settings(&candidate, id.as_deref(), &source)?;
    let mut checked: AppConfig =
        serde_json::from_value(effective).map_err(|e| AppError::invalid_argument(e.to_string()))?;
    validation::validate(&mut checked)?;
    // Persist only after the common apply/recovery path confirms the operation.
    super::apply_candidate(handle, state, previous, candidate).await
}
