pub mod commands;
pub mod config;
pub mod errors;
pub mod events;
pub mod kernel;
pub mod lifecycle;
pub mod models;
pub mod services;
pub mod state;

use crate::commands::app_config as app_config_commands;
use crate::commands::capability as capability_commands;
use crate::commands::core as core_commands;
use crate::commands::core_config as core_config_commands;
use crate::commands::core_process as core_process_commands;
use crate::commands::gui_connection as gui_connection_commands;
use crate::commands::gui_core as gui_core_commands;
use crate::commands::gui_events as gui_events_commands;
use crate::commands::gui_self_test as gui_self_test_commands;
use crate::commands::kernel_version as kernel_version_commands;
use crate::commands::logs as logs_commands;
use crate::commands::proxy_config as proxy_config_commands;
use crate::commands::proxy_mode as proxy_mode_commands;
use crate::commands::rule_set as rule_set_commands;
use crate::commands::subscription as subscription_commands;
use crate::commands::system_proxy as system_proxy_commands;
use crate::lifecycle::phases;
use crate::services::{core_process, system_proxy_guard};
use crate::kernel::adapter::KernelAdapter;
use crate::state::app_state::AppState;
use tauri::Manager;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Phase 1–2: Guard + Config (runs before Tauri builder) ──
    let (mut lifecycle, startup_data) = phases::build_builtin();
    lifecycle.startup().expect("lifecycle startup failed");

    let data = startup_data
        .lock()
        .expect("startup data lock")
        .take()
        .expect("startup data should be populated by Config phase");

    // ── Phase 3: State — construct AppState from loaded data ──
    eprintln!("[ZNet] lifecycle: entering phase state");
    let app_state = AppState::with_domain_data(
        data.app_config,
        data.domain_data.proxy_configs,
        data.domain_data.subscriptions,
        data.domain_data.rule_sets,
        data.logs,
    );
    eprintln!("[ZNet] lifecycle:   → app_state");

    // Register core-process shutdown guard: stop core on exit.
    let shutdown_coord = lifecycle.shutdown_coordinator_mut();
    shutdown_coord.register(
        lifecycle::Phase::Runtime,
        "stop_core_process",
        Box::new(|| {
            // ManagedCoreProcess::Drop kills + waits on the child process.
            // This guard runs first so logs appear in correct order.
            eprintln!("[ZNet] shutdown: stopping core process (via Drop)");
        }),
    );
    shutdown_coord.register(
        lifecycle::Phase::Guard,
        "system_proxy_cleanup",
        Box::new(move || {
            // Ensure system proxy is disabled on clean exit
            system_proxy_guard::disable_with_guard().ok();
        }),
    );

    // ── Phase 4–5: Register + Runtime (inside Tauri builder) ──
    tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // ── Phase 4: Register commands ──
        .invoke_handler(tauri::generate_handler![
            core_commands::core_ipc_default_endpoint,
            core_commands::core_status,
            core_commands::core_ipc_ping,
            core_commands::core_ipc_query,
            core_commands::core_ipc_command,
            core_commands::core_ipc_request,
            core_commands::core_get_capabilities,
            core_commands::core_get_health,
            core_commands::core_get_config,
            core_commands::core_get_runtime,
            core_commands::core_get_stats,
            core_commands::core_get_policies,
            core_commands::core_select_policy,
            core_commands::core_probe_policy,
            core_commands::core_close_flow,
            core_commands::core_validate_config,
            core_commands::core_events_start,
            core_commands::core_events_stop,
            core_config_commands::core_config_get,
            core_process_commands::core_process_status,
            core_process_commands::core_process_start,
            core_process_commands::core_process_stop,
            core_config_commands::core_config_export_active,
            core_config_commands::core_download_latest,
            gui_core_commands::gui_core_overview,
            gui_core_commands::gui_core_health,
            gui_core_commands::gui_zero_capabilities,
            gui_core_commands::gui_traffic_stats,
            gui_core_commands::gui_traffic_snapshot,
            gui_core_commands::gui_policy_groups,
            gui_core_commands::gui_proxy_nodes,
            gui_core_commands::gui_select_policy,
            gui_core_commands::gui_probe_target,
            gui_core_commands::gui_client_probe_node,
            gui_core_commands::gui_client_probe_start,
            gui_core_commands::gui_connections,
            gui_core_commands::gui_connection_detail,
            gui_core_commands::gui_close_connection,
            gui_core_commands::gui_dns_status,
            gui_core_commands::gui_tun_status,
            gui_core_commands::gui_tun_enable,
            gui_core_commands::gui_tun_disable,
            gui_core_commands::gui_stack_status,
            gui_core_commands::gui_rule_status,
            gui_connection_commands::gui_connection_status,
            gui_connection_commands::gui_connect,
            gui_connection_commands::gui_disconnect,
            gui_events_commands::gui_events_start,
            gui_events_commands::gui_events_stop,
            gui_self_test_commands::gui_self_test_snapshot,
            proxy_mode_commands::gui_proxy_mode_status,
            proxy_mode_commands::gui_set_proxy_mode,
            app_config_commands::app_config_get,
            app_config_commands::app_config_update,
            proxy_config_commands::proxy_config_list,
            proxy_config_commands::proxy_config_get,
            proxy_config_commands::proxy_config_upsert,
            proxy_config_commands::proxy_config_import,
            proxy_config_commands::proxy_config_set_active,
            proxy_config_commands::proxy_config_remove,
            subscription_commands::subscription_list,
            subscription_commands::subscription_get,
            subscription_commands::subscription_upsert,
            subscription_commands::subscription_sync,
            subscription_commands::subscription_remove,
            rule_set_commands::rule_set_list,
            rule_set_commands::rule_set_get,
            rule_set_commands::rule_set_upsert,
            rule_set_commands::rule_set_remove,
            logs_commands::logs_list,
            logs_commands::logs_append,
            logs_commands::logs_clear,
            capability_commands::gui_capabilities_snapshot,
            capability_commands::gui_interaction_surface_snapshot,
            system_proxy_commands::system_proxy_enable,
            system_proxy_commands::system_proxy_disable,
            system_proxy_commands::system_proxy_status,
            kernel_version_commands::kernel_list_versions,
            kernel_version_commands::kernel_install_version,
            kernel_version_commands::kernel_detect_version,
        ])
        // ── Phase 5: Runtime — tray, auto-start, window ──
        .setup(|app| {
            let auto_start = app
                .state::<AppState>()
                .app_config()
                .lock()
                .map(|config| config.core.auto_start)
                .unwrap_or(false);
            if auto_start {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let adapter = crate::kernel::zero::ZeroAdapter::new();
                    let opts = crate::services::core_config::ipc_options_from_app_config(
                        &state.app_config().lock().map(|c| c.core.clone()).unwrap_or_default()
                    );
                    if adapter.readiness_health(opts).await.is_ok() {
                        return;
                    }
                    drop(state);

                    let app_handle_start = app_handle.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let state = app_handle_start.state::<AppState>();
                        let _ = core_process::start(app_handle_start.clone(), state);
                    })
                    .await;
                });
            }

            // System tray
            let show_item = tauri::menu::MenuItemBuilder::new("显示")
                .id("show")
                .build(app)?;
            let quit_item = tauri::menu::MenuItemBuilder::new("退出")
                .id("quit")
                .build(app)?;

            let tray_menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &show_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            let _tray_menu = TrayIconBuilder::with_id("main-tray")
                .tooltip("ZNet Sink")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button,
                        button_state,
                        ..
                    } = event
                    {
                        if button == MouseButton::Left && button_state == MouseButtonState::Up {
                            if let Some(window) = tray.app_handle().get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // ── Shutdown: runs after Tauri event loop exits ──
    lifecycle.shutdown();
}
