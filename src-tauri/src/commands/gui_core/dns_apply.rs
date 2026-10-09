use super::GuiDnsSettingsInput;
use crate::errors::AppResult;
use crate::models::{
    app_config::{AppConfigPatch, AppDnsConfigPatch},
    core_process::CoreProcessState,
};
use crate::services::{common, core_process};
use crate::state::app_state::AppState;
use serde_json::Value;
use tauri::{AppHandle, State};

#[cfg(test)]
mod transaction;

pub(super) async fn apply(
    handle: AppHandle,
    state: State<'_, AppState>,
    input: GuiDnsSettingsInput,
) -> AppResult<Value> {
    let previous = common::lock(state.app_config(), "app_config")?.clone();
    let (id, source) = crate::configuration::local_edits::context(state.inner())?;
    let candidate = crate::configuration::local_edits::prepare_patch(
        &previous,
        id.as_deref(),
        &source,
        AppConfigPatch {
            dns: Some(AppDnsConfigPatch {
                enabled: Some(input.enabled),
                config: Some(input.config),
                dns_hijack: Some(input.enabled && input.dns_hijack),
            }),
            ..Default::default()
        },
    )?;
    crate::commands::app_config::apply_candidate(handle, state.clone(), previous, candidate)
        .await?;
    let applied = core_process::refresh_status(state.inner())?.state == CoreProcessState::Running;
    Ok(serde_json::json!({"ok": true, "applied": applied}))
}
