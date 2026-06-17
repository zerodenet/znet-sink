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
use crate::commands::debug as debug_commands;
use crate::commands::gui_self_test as gui_self_test_commands;
use crate::commands::kernel_version as kernel_version_commands;
use crate::commands::logs as logs_commands;
use crate::commands::proxy_config as proxy_config_commands;
use crate::commands::proxy_mode as proxy_mode_commands;
use crate::commands::rule_set as rule_set_commands;
use crate::commands::subscription as subscription_commands;
use crate::commands::system_proxy as system_proxy_commands;
use crate::lifecycle::phases;
use crate::services::{core_process, local_proxy, system_proxy_guard};
use crate::state::app_state::AppState;
use tauri::{Emitter, Manager};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

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

fn tray_start_core(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _ = core_process::start(app.clone(), state);
    });
}

fn tray_stop_core(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _ = core_process::stop(state);
    });
}

fn tray_restart_core(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _ = core_process::stop(state.clone());
        let _ = core_process::start(app.clone(), state);
    });
}

fn tray_enable_system_proxy(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _ = core_process::start(app.clone(), state.clone());
        let host = state
            .app_config()
            .lock()
            .map(|config| config.local_proxy.host.clone())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = state
            .app_config()
            .lock()
            .map(|config| config.local_proxy.port)
            .unwrap_or(7890);
        let _ = local_proxy::wait_until_listening(&host, port);
        let _ = system_proxy_guard::enable_with_guard(&host, port);
    });
}

fn tray_disable_system_proxy(_app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = system_proxy_guard::disable_with_guard();
    });
}

/// Holds references to the status-dependent tray menu items so we can
/// toggle their enabled state at runtime (e.g. disable "启动内核" while
/// the kernel is already running). Stored via `app.manage()`.
struct TrayMenuItems {
    start_core: tauri::menu::MenuItem<tauri::Wry>,
    stop_core: tauri::menu::MenuItem<tauri::Wry>,
    restart_core: tauri::menu::MenuItem<tauri::Wry>,
    enable_proxy: tauri::menu::MenuItem<tauri::Wry>,
    disable_proxy: tauri::menu::MenuItem<tauri::Wry>,
}

/// Update the tray icon tooltip and the enabled state of status-dependent
/// menu items based on the current kernel / proxy state.
///
/// Called from the frontend whenever connection or process state changes
/// so the system-tray icon always reflects reality (e.g.
/// "ZNet Sink · 服务中") without the user opening the window.
#[tauri::command]
fn tray_update_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, TrayMenuItems>,
    running: bool,
    connected: bool,
) {
    let status_label = if connected {
        "服务中"
    } else if running {
        "内核监听中"
    } else {
        "已停止"
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(format!("ZNet Sink · {status_label}")));
    }

    // Mirror the actionable state into the menu so the user can't pick
    // an action that would no-op (e.g. "启动内核" while already running).
    let _ = state.start_core.set_enabled(!running);
    let _ = state.stop_core.set_enabled(running);
    let _ = state.restart_core.set_enabled(running);
    let _ = state.enable_proxy.set_enabled(!connected);
    let _ = state.disable_proxy.set_enabled(connected);
}

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
    let shutdown_flag = app_state.shutting_down_handle();
    shutdown_coord.register(
        lifecycle::Phase::Runtime,
        "stop_core_process",
        Box::new(|| {
            // Explicitly kill the kernel so it exits with the GUI. Relying
            // on ManagedCoreProcess::Drop alone is unreliable — Drop may not
            // run (external kernel, or process exit without unwinding).
            eprintln!("[ZNet] shutdown: stopping core process");
            core_process::kill_core_default();
        }),
    );
    // Registered AFTER stop_core_process so LIFO ordering runs it FIRST:
    // the watchdog must see the shutdown flag before we tear the process
    // down, otherwise it would immediately try to restart the kernel.
    shutdown_coord.register(
        lifecycle::Phase::Runtime,
        "mark_shutting_down",
        Box::new(move || {
            shutdown_flag.store(true, std::sync::atomic::Ordering::SeqCst);
            eprintln!("[ZNet] shutdown: marking shutdown (watchdog will stop restarting)");
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
            core_process_commands::core_process_restart,
            core_config_commands::core_config_export_active,
            core_config_commands::core_download_latest,
            gui_core_commands::gui_core_overview,
            gui_core_commands::gui_core_health,
            gui_core_commands::gui_zero_capabilities,
            gui_core_commands::gui_traffic_stats,
            gui_core_commands::gui_traffic_snapshot,
            gui_core_commands::gui_policy_groups,
            gui_core_commands::gui_config_policy_groups,
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
            gui_core_commands::gui_apply_config,
            gui_core_commands::gui_validate_config,
            gui_core_commands::gui_plan_apply_config,
            gui_core_commands::gui_set_mode,
            gui_core_commands::gui_probe_policy,
            gui_core_commands::gui_dns_lookup,
            gui_core_commands::gui_trace_route,
            gui_core_commands::gui_recent_connections,
            gui_core_commands::gui_sinks,
            gui_core_commands::gui_diagnostics,
            gui_connection_commands::gui_connection_status,
            gui_connection_commands::gui_connect,
            gui_connection_commands::gui_disconnect,
            gui_events_commands::gui_events_start,
            gui_events_commands::gui_events_stop,
            debug_commands::gui_debug_frames,
            debug_commands::gui_debug_clear,
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
            subscription_commands::subscription_sync_all,
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
            tray_update_status,
        ])
        // ── Phase 5: Runtime — tray, kernel lifecycle, window ──
        .setup(|app| {
            // Always check kernel health on startup. If the kernel is already
            // running (e.g. external daemon), just connect. If not, try to
            // start a managed kernel when auto_start is enabled (default).
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let base_opts = crate::services::core_config::ipc_options_from_app_config(
                        &state.app_config().lock().map(|c| c.core.clone()).unwrap_or_default()
                    );

                    // Fast probe: try a ping with a 200ms timeout.  On local
                    // IPC this should connect in sub-ms time if the kernel is
                    // alive.  A timeout means the pipe is a stale leftover
                    // from a crashed/killed previous session — clean up and
                    // start fresh.
                    let probe_opts = crate::models::core::CoreIpcOptions {
                        timeout_ms: Some(200),
                        ..base_opts.clone()
                    };
                    // Use a short-lived connection for the probe — the kernel
                    // closes non-subscribe connections after responding, and we
                    // don't want to poison the global multiplexed connection.
                    let kernel_alive = {
                        let frame = serde_json::json!({"type":"ping"});
                        let endpoint = crate::kernel::protocol::endpoint_from_options(Some(&probe_opts)).ok();
                        let timeout = crate::kernel::protocol::timeout_from_options(Some(&probe_opts)).ok();
                        match (endpoint, timeout) {
                            (Some(ep), Some(to)) => {
                                let frame_bytes = crate::kernel::transport::serialize_frame(&frame).ok();
                                frame_bytes.and_then(|fb| {
                                    crate::kernel::transport::send_json_line_request(ep, fb, to).ok()
                                }).is_some()
                            }
                            _ => false,
                        }
                    };

                    // Extract values needed for decision-making before
                    // consuming `state`.  All three are needed regardless of
                    // which branch we take below.
                    let core_config = state
                        .app_config()
                        .lock()
                        .map(|c| c.core.clone())
                        .unwrap_or_default();
                    let auto_start = core_config.auto_start;
                    let configured_path = core_config.executable_path.clone();

                    if kernel_alive {
                        // Check whether the running kernel matches the
                        // configured executable path.  If the user changed
                        // the path between sessions the old kernel is still
                        // listening on the pipe but is no longer the binary
                        // the user intended.
                        let snapshot = crate::services::core_config::snapshot_from_config(
                            &core_config,
                        ).ok();

                        let path_matches = match (&configured_path, &snapshot) {
                            (Some(configured), Some(snap)) => {
                                snap.executable_path.as_deref() == Some(configured.as_str())
                                    || configured.is_empty()
                            }
                            _ => true, // no custom path → any running kernel is fine
                        };

                        if !path_matches {
                            eprintln!("[ZNet] kernel alive but path mismatch, restarting");
                            // Use fresh State — the outer `state` is not `Copy`.
                            let _ = core_process::stop(app_handle.state::<AppState>());
                            crate::kernel::connection::reset();
                            // Fall through to start the configured kernel.
                        } else {
                            eprintln!("[ZNet] kernel already running (fast probe ok), connecting");

                            // Update the process state so the UI reflects the
                            // actual kernel status.  Without this the UI shows
                            // "not started" even though the kernel is alive,
                            // which confuses users and also means stop() won't
                            // know to kill the external process.
                            {
                                let mut process = match state.core_process().lock() {
                                    Ok(p) => p,
                                    Err(_) => return,
                                };
                                process.status.state =
                                    crate::models::core_process::CoreProcessState::Running;
                                process.status.kernel = "zero".to_string();
                                // No child — we don't own this process, so
                                // stop() will fall through to kill_external().
                            }
                            return;
                        }
                    }

                    // Kernel not running (or was restarted due to path mismatch).
                    if !auto_start {
                        eprintln!("[ZNet] auto_start disabled, not starting kernel");
                        return;
                    }

                    let app_handle_start = app_handle.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let state = app_handle_start.state::<AppState>();
                        let _ = core_process::start(app_handle_start.clone(), state);
                    })
                    .await;
                });
            }

            // System tray
            let show_item = tauri::menu::MenuItemBuilder::new("显示/隐藏")
                .id("show")
                .build(app)?;
            let enable_proxy_item = tauri::menu::MenuItemBuilder::new("开启系统代理")
                .id("enable_proxy")
                .build(app)?;
            let disable_proxy_item = tauri::menu::MenuItemBuilder::new("关闭系统代理")
                .id("disable_proxy")
                .build(app)?;
            let start_core_item = tauri::menu::MenuItemBuilder::new("启动内核")
                .id("start_core")
                .build(app)?;
            let stop_core_item = tauri::menu::MenuItemBuilder::new("停止内核")
                .id("stop_core")
                .build(app)?;
            let restart_core_item = tauri::menu::MenuItemBuilder::new("重启内核")
                .id("restart_core")
                .build(app)?;
            let settings_item = tauri::menu::MenuItemBuilder::new("设置")
                .id("settings")
                .build(app)?;
            let quit_item = tauri::menu::MenuItemBuilder::new("退出")
                .id("quit")
                .build(app)?;

            let tray_menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &show_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &enable_proxy_item,
                    &disable_proxy_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &start_core_item,
                    &stop_core_item,
                    &restart_core_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &settings_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            // Hold references to the status-dependent items so
            // `tray_update_status` can toggle their enabled state.
            app.manage(TrayMenuItems {
                start_core: start_core_item,
                stop_core: stop_core_item,
                restart_core: restart_core_item,
                enable_proxy: enable_proxy_item,
                disable_proxy: disable_proxy_item,
            });

            let _tray_menu = TrayIconBuilder::with_id("main-tray")
                .tooltip("ZNet Sink · 已停止")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => toggle_main_window(app),
                    "enable_proxy" => tray_enable_system_proxy(app.clone()),
                    "disable_proxy" => tray_disable_system_proxy(app.clone()),
                    "start_core" => tray_start_core(app.clone()),
                    "stop_core" => tray_stop_core(app.clone()),
                    "restart_core" => tray_restart_core(app.clone()),
                    "settings" => open_main_window_route(app, "settings", Some("general")),
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
                            toggle_main_window(tray.app_handle());
                        }
                    }
                })
                .build(app)?;

            // Spawn the subscription auto-sync scheduler. It re-syncs
            // any enabled subscription that has an update interval once
            // that interval elapses. The first pass is delayed to let
            // the kernel and network come up.
            crate::services::subscription::spawn_auto_sync_scheduler(app.handle().clone());

            // Spawn the traffic sampler so the overview chart updates live —
            // the kernel doesn't push traffic events on its own (TODO P5).
            crate::services::traffic_sampler::spawn(app.handle().clone());

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
