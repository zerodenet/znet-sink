use tauri::{AppHandle, State};

use crate::errors::AppResult;
use crate::kernel::connection;
use crate::models::{core::CoreIpcOptions, gui_core::GuiEventSubscription};
use crate::services::common::lock;
use crate::services::{core_config, gui_events};
use crate::state::app_state::AppState;

#[tauri::command]
pub fn gui_events_start(
    app: AppHandle,
    state: State<'_, AppState>,
    events: Option<Vec<String>>,
    options: Option<CoreIpcOptions>,
) -> AppResult<GuiEventSubscription> {
    let options = resolve_options(&state, options)?;
    let binding = znet_engine_client::Binding {
        endpoint: crate::kernel::protocol::endpoint_from_options(options.as_ref())?,
        timeout: crate::kernel::protocol::timeout_from_options(options.as_ref())?,
    };
    let subscription = state.observations().begin(binding);
    gui_events::start(app, subscription, events)
}

#[tauri::command]
pub fn gui_events_stop(state: State<'_, AppState>) -> u64 {
    let (generation, binding) = state.observations().stop();
    if let Some(binding) = binding {
        // Retire only the observation endpoint; raw diagnostics may be connected
        // to another engine. Never reset that unrelated transport.
        connection::reset_endpoint(&binding.endpoint);
    }
    generation
}

fn resolve_options(
    state: &State<'_, AppState>,
    options: Option<CoreIpcOptions>,
) -> AppResult<Option<CoreIpcOptions>> {
    if options
        .as_ref()
        .and_then(|options| options.socket.as_deref())
        .is_some_and(|socket| !socket.trim().is_empty())
    {
        return Ok(options);
    }

    let mut resolved = default_options(state)?;
    if let Some(timeout_ms) = options.and_then(|options| options.timeout_ms) {
        resolved.timeout_ms = Some(timeout_ms);
    }
    Ok(Some(resolved))
}

fn default_options(state: &State<'_, AppState>) -> AppResult<CoreIpcOptions> {
    let config = lock(state.app_config(), "app_config")?.core.clone();
    Ok(core_config::ipc_options_from_app_config(&config))
}
