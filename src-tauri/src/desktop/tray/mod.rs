//! Tray presentation and user-intent dispatch for the desktop shell.

mod environment;
mod menu;
mod presentation;

use crate::state::app_state::AppState;
pub(crate) use menu::install;
use presentation::tray_presentation;
use tauri::{Emitter, Manager};

fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

fn open_main_window_route(app: &tauri::AppHandle, tab: &str, section: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit(
        "app:navigate",
        serde_json::json!({ "tab": tab, "section": section }),
    );
}

fn emit_tray_action(app: &tauri::AppHandle, action: &str) {
    if let Err(error) = app.emit("app:tray-action", serde_json::json!({ "action": action })) {
        crate::services::file_logger::line(&format!(
            "tray: failed to dispatch action {action}: {error}"
        ));
    }
}

/// Holds references to the status-dependent tray menu items so their labels,
/// checked state, and availability can track the live runtime state.
pub(crate) struct TrayMenuItems {
    status: tauri::menu::MenuItem<tauri::Wry>,
    profile: tauri::menu::MenuItem<tauri::Wry>,
    toggle_proxy: tauri::menu::MenuItem<tauri::Wry>,
    system_proxy: tauri::menu::CheckMenuItem<tauri::Wry>,
    tun: tauri::menu::CheckMenuItem<tauri::Wry>,
    restart_core: tauri::menu::MenuItem<tauri::Wry>,
}

/// Update the tray icon tooltip and the enabled state of status-dependent
/// menu items based on the current kernel / proxy state.
///
/// Called from the frontend whenever connection or process state changes
/// so the system-tray icon always reflects reality (e.g.
/// "ZNet Sink · 服务中") without the user opening the window.
#[tauri::command]
pub(crate) fn tray_update_status(
    app: tauri::AppHandle,
    menu: tauri::State<'_, TrayMenuItems>,
    domain: tauri::State<'_, AppState>,
    running: bool,
    system_proxy_enabled: bool,
    tun_enabled: bool,
) {
    let ui_mode = domain
        .app_config()
        .lock()
        .map(|config| config.ui.ui_mode.clone())
        .unwrap_or_else(|_| "lite".to_string());
    let profile_name = domain.proxy_configs().lock().ok().and_then(|profiles| {
        profiles
            .iter()
            .find(|profile| profile.active)
            .map(|profile| profile.name.clone())
    });
    let presentation = tray_presentation(&ui_mode, running, system_proxy_enabled, tun_enabled);

    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(format!("ZNet Sink · {}", presentation.status_label)));
    }

    let _ = menu
        .status
        .set_text(format!("状态：{}", presentation.status_label));
    let _ = menu.profile.set_text(format!(
        "当前配置：{}",
        profile_name.as_deref().unwrap_or("未选择")
    ));
    let _ = menu.toggle_proxy.set_text(presentation.action_label);
    // A profile is required to start capture, but never prevent users from
    // turning off a capture session that is already active.
    let _ = menu
        .toggle_proxy
        .set_enabled(profile_name.is_some() || presentation.capture_enabled);
    let _ = menu.system_proxy.set_checked(system_proxy_enabled);
    let _ = menu.tun.set_checked(tun_enabled);
    // Lite mode treats system proxy + TUN as one transaction. Individual
    // switches are advanced controls and stay available only in Pro mode.
    let _ = menu
        .system_proxy
        .set_enabled(presentation.individual_controls_enabled);
    let _ = menu
        .tun
        .set_enabled(presentation.individual_controls_enabled);
    let _ = menu.restart_core.set_enabled(running);
}

#[cfg(test)]
mod tests;
