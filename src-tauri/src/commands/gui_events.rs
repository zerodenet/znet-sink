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
    let generation = state.next_gui_event_generation();
    let options = resolve_options(&state, options)?;
    gui_events::start(
        app,
        state.gui_event_generation(),
        generation,
        events,
        options,
    )
}

#[tauri::command]
pub fn gui_events_stop(state: State<'_, AppState>) -> u64 {
    // Advancing the GUI generation stops the current forwarder, but that alone
    // does not create a new Zero subscription: `get_or_connect` intentionally
    // reuses the still-alive multiplexed IPC connection whose initial
    // `subscribe` frame belongs to the previous runtime/event source. Close the
    // ZNet-Sink-owned multiplexed connection as part of the event-stream stop
    // boundary so the next gui_events_start performs a fresh pipe connect and
    // subscribe handshake. Other IPC callers recover through get_or_connect.
    let generation = state.next_gui_event_generation();
    connection::reset();
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
