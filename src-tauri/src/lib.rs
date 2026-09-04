pub mod client_core;
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
use crate::commands::app_update as app_update_commands;
use crate::commands::capability as capability_commands;
use crate::commands::core as core_commands;
use crate::commands::core_config as core_config_commands;
use crate::commands::core_process as core_process_commands;
use crate::commands::debug as debug_commands;
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
use crate::services::{core_process, network_probe, system_proxy_guard};
use crate::state::app_state::AppState;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "windows")]
use crate::services::local_proxy;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NEW_CONSOLE: u32 = 0x00000010;

fn proxy_environment_no_proxy(bypass: &[String]) -> String {
    bypass
        .iter()
        .map(|entry| match entry.trim().to_ascii_lowercase().as_str() {
            // ProxyOverride accepts Windows wildcards, while most CLI tools
            // understand IP ranges in NO_PROXY as CIDR blocks.
            "<local>" => "localhost".to_string(),
            "127.*" => "127.0.0.0/8".to_string(),
            "[::1]" => "::1".to_string(),
            "10.*" => "10.0.0.0/8".to_string(),
            "192.168.*" => "192.168.0.0/16".to_string(),
            value if value.starts_with("172.") && value.ends_with(".*") => {
                let second_octet = value
                    .trim_start_matches("172.")
                    .trim_end_matches(".*")
                    .parse::<u8>();
                match second_octet {
                    Ok(octet @ 16..=31) => format!("172.{octet}.0.0/16"),
                    _ => entry.trim().to_string(),
                }
            }
            _ => entry.trim().to_string(),
        })
        .filter(|entry| !entry.is_empty())
        .fold(Vec::<String>::new(), |mut entries, entry| {
            if !entries
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&entry))
            {
                entries.push(entry);
            }
            entries
        })
        .join(",")
}

fn proxy_environment_command(host: &str, port: u16, bypass: &[String]) -> String {
    let http_url = format!("http://{host}:{port}");
    let socks_url = format!("socks5h://{host}:{port}");
    let no_proxy = proxy_environment_no_proxy(bypass);
    if cfg!(target_os = "windows") {
        format!(
            "$env:HTTP_PROXY='{http_url}'; $env:HTTPS_PROXY='{http_url}'; \
             $env:ALL_PROXY='{socks_url}'; $env:NO_PROXY='{no_proxy}'; \
             $env:http_proxy='{http_url}'; $env:https_proxy='{http_url}'; \
             $env:all_proxy='{socks_url}'; $env:no_proxy='{no_proxy}'"
        )
    } else {
        format!(
            "export HTTP_PROXY='{http_url}' HTTPS_PROXY='{http_url}' \
             ALL_PROXY='{socks_url}' NO_PROXY='{no_proxy}' \
             http_proxy='{http_url}' https_proxy='{http_url}' \
             all_proxy='{socks_url}' no_proxy='{no_proxy}'"
        )
    }
}

fn tray_copy_proxy_environment(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let endpoint = state.app_config().lock().map(|config| {
        (
            config.local_proxy.host.clone(),
            config.local_proxy.port,
            config.local_proxy.bypass.clone(),
        )
    });
    if let Ok((host, port, bypass)) = endpoint {
        let _ = app
            .clipboard()
            .write_text(proxy_environment_command(&host, port, &bypass));
    }
}

#[cfg(target_os = "windows")]
fn spawn_proxy_terminal(
    host: &str,
    port: u16,
    bypass: &[String],
) -> std::io::Result<(String, u32)> {
    let http_url = format!("http://{host}:{port}");
    let socks_url = format!("socks5h://{host}:{port}");
    let no_proxy = proxy_environment_no_proxy(bypass);
    let mut last_not_found = None;

    for program in ["pwsh.exe", "powershell.exe"] {
        let mut command = Command::new(program);
        command
            // Tauri is built as a Windows GUI process, so child console
            // applications do not reliably receive a visible console unless
            // one is explicitly requested.
            .creation_flags(CREATE_NEW_CONSOLE)
            // The Zero mixed listener accepts both HTTP CONNECT and SOCKS5.
            // Protocol-specific variables maximize compatibility with package
            // managers, while ALL_PROXY covers tools that support SOCKS5.
            .env("HTTP_PROXY", &http_url)
            .env("HTTPS_PROXY", &http_url)
            .env("ALL_PROXY", &socks_url)
            .env("NO_PROXY", &no_proxy)
            .env("http_proxy", &http_url)
            .env("https_proxy", &http_url)
            .env("all_proxy", &socks_url)
            .env("no_proxy", &no_proxy)
            .args([
                "-NoLogo",
                "-NoExit",
                "-Command",
                "$Host.UI.RawUI.WindowTitle = 'ZNet Sink Terminal'; Write-Host ('Proxy enabled: ' + $env:HTTP_PROXY); Write-Host ('SOCKS5 fallback: ' + $env:ALL_PROXY)",
            ]);

        match command.spawn() {
            Ok(child) => return Ok((program.to_string(), child.id())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_not_found = Some(error);
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_not_found.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "PowerShell executable was not found",
        )
    }))
}

#[cfg(target_os = "windows")]
fn tray_open_proxy_terminal(app: tauri::AppHandle) {
    crate::services::file_logger::line("tray: open terminal requested");

    tauri::async_runtime::spawn_blocking(move || {
        crate::services::file_logger::line("tray: preparing terminal with proxy environment");

        let state = app.state::<AppState>();
        let _operation = state.proxy_config_operation().blocking_lock();

        if let Err(error) = core_process::start(app.clone(), state.clone()) {
            crate::services::file_logger::line(&format!(
                "tray: failed to start core before opening terminal: {}",
                error.message
            ));
            return;
        }
        crate::services::file_logger::line("tray: core is ready for terminal");

        let endpoint = state.app_config().lock().map(|config| {
            (
                config.local_proxy.host.clone(),
                config.local_proxy.port,
                config.local_proxy.bypass.clone(),
            )
        });
        let Ok((host, port, bypass)) = endpoint else {
            crate::services::file_logger::line(
                "tray: failed to read local proxy endpoint for terminal",
            );
            return;
        };
        crate::services::file_logger::line(&format!(
            "tray: waiting for terminal proxy endpoint {host}:{port}"
        ));

        if let Err(error) = local_proxy::wait_until_listening(&host, port) {
            crate::services::file_logger::line(&format!(
                "tray: local proxy is not ready for terminal: {}",
                error.message
            ));
            return;
        }
        crate::services::file_logger::line("tray: terminal proxy endpoint is listening");

        match spawn_proxy_terminal(&host, port, &bypass) {
            Ok((program, pid)) => {
                crate::services::file_logger::line(&format!(
                    "tray: opened terminal using {program}, pid={pid}"
                ));
            }
            Err(error) => {
                crate::services::file_logger::line(&format!(
                    "tray: failed to open terminal with proxy environment: {error}"
                ));
            }
        }
    });
}

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
struct TrayMenuItems {
    status: tauri::menu::MenuItem<tauri::Wry>,
    profile: tauri::menu::MenuItem<tauri::Wry>,
    toggle_proxy: tauri::menu::MenuItem<tauri::Wry>,
    system_proxy: tauri::menu::CheckMenuItem<tauri::Wry>,
    tun: tauri::menu::CheckMenuItem<tauri::Wry>,
    restart_core: tauri::menu::MenuItem<tauri::Wry>,
}

#[derive(Debug, PartialEq, Eq)]
struct TrayPresentation {
    status_label: &'static str,
    action_label: &'static str,
    capture_enabled: bool,
    individual_controls_enabled: bool,
}

fn tray_presentation(
    ui_mode: &str,
    running: bool,
    system_proxy_enabled: bool,
    tun_enabled: bool,
) -> TrayPresentation {
    let lite_mode = ui_mode.eq_ignore_ascii_case("lite");
    let capture_enabled = if lite_mode {
        system_proxy_enabled || tun_enabled
    } else {
        system_proxy_enabled
    };
    let status_label = if system_proxy_enabled && tun_enabled {
        "系统代理 + TUN"
    } else if tun_enabled {
        "TUN 运行中"
    } else if system_proxy_enabled {
        "系统代理运行中"
    } else if running {
        "待机"
    } else {
        "已停止"
    };

    TrayPresentation {
        status_label,
        action_label: if capture_enabled {
            "关闭代理"
        } else {
            "开启代理"
        },
        capture_enabled,
        individual_controls_enabled: running && !lite_mode,
    }
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // File logger + panic hook — first thing, so every later phase (and any
    // crash) lands in <data_dir>/logs/gui.log.jsonl.
    crate::services::file_logger::init();

    // ── Phase 1–2: Guard + Config (runs before Tauri builder) ──
    let (mut lifecycle, startup_data) = phases::build_builtin();
    lifecycle.startup().expect("lifecycle startup failed");

    let data = startup_data
        .lock()
        .expect("startup data lock")
        .take()
        .expect("startup data should be populated by Config phase");

    // ── Phase 3: State — construct AppState from loaded data ──
    crate::services::file_logger::line("lifecycle: entering phase state");
    let app_state = AppState::with_domain_data(
        data.app_config,
        data.domain_data.proxy_configs,
        data.domain_data.subscriptions,
        data.domain_data.rule_sets,
        data.logs,
    );
    if let Ok(observations) = crate::services::probe_history::load() {
        app_state.restore_client_probe_observations(observations);
    }
    crate::services::file_logger::line("lifecycle:   → app_state");

    // 0 = running, 1 = graceful cleanup in progress, 2 = cleanup complete.
    // The lifecycle fallback reads the same state after the event loop exits.
    let shutdown_stage = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));

    // Mark shutdown before lifecycle teardown so background watchdogs cannot
    // restart the managed child. Normal cleanup runs in the Tauri exit event;
    // on an abrupt GUI death the inherited lifetime pipe closes in the OS.
    let shutdown_coord = lifecycle.shutdown_coordinator_mut();
    let shutdown_flag = app_state.shutting_down_handle();
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
        Box::new({
            let cleanup = app_state
                .app_config()
                .lock()
                .map(|config| config.core.cleanup_proxy_on_exit)
                .unwrap_or(true);
            move || {
                if cleanup {
                    // Restore the user's proxy only when the preference is enabled.
                    system_proxy_guard::disable_with_guard().ok();
                }
            }
        }),
    );

    crate::services::file_logger::line("runtime: entering register/runtime phase");
    // ── Phase 4–5: Register + Runtime (inside Tauri builder) ──
    let app = tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
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
            core_process_commands::core_process_restart,
            core_config_commands::core_config_export_active,
            core_config_commands::core_download_latest,
            gui_core_commands::gui_core_overview,
            gui_core_commands::gui_client_core_snapshot,
            gui_core_commands::gui_node_screen_snapshot,
            gui_core_commands::gui_probe_job_start,
            gui_core_commands::gui_probe_job_get,
            gui_core_commands::gui_probe_job_list,
            gui_core_commands::gui_probe_job_cancel,
            gui_core_commands::gui_core_health,
            gui_core_commands::gui_zero_capabilities,
            gui_core_commands::gui_traffic_stats,
            gui_core_commands::gui_traffic_snapshot,
            gui_core_commands::gui_policy_groups,
            gui_core_commands::gui_config_policy_groups,
            gui_core_commands::gui_proxy_nodes,
            gui_core_commands::gui_select_policy,
            gui_core_commands::gui_probe_target,
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
            // DNS/Fake-IP is a client-global override applied to the effective Zero config.
            gui_core_commands::gui_apply_dns_config,
            gui_core_commands::gui_validate_config,
            gui_core_commands::gui_validate_dns_config,
            gui_core_commands::gui_inspect_dns_effective_config,
            gui_core_commands::gui_set_mode,
            gui_core_commands::gui_probe_policy,
            gui_core_commands::gui_dns_lookup,
            gui_core_commands::gui_dns_cache,
            gui_core_commands::gui_fakeip_lookup,
            gui_core_commands::gui_clear_fake_ip,
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
            app_config_commands::app_config_apply_tun,
            app_config_commands::app_config_export_kernel_settings,
            app_config_commands::app_config_import_kernel_settings,
            app_update_commands::app_check_release,
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
            subscription_commands::subscription_remove_preview,
            subscription_commands::subscription_remove,
            rule_set_commands::rule_set_list,
            rule_set_commands::rule_set_get,
            rule_set_commands::rule_set_upsert,
            rule_set_commands::rule_set_remove,
            rule_set_commands::rule_set_update,
            rule_set_commands::rule_set_update_all,
            rule_set_commands::rule_set_update_builtins,
            rule_set_commands::rule_set_kernel_payloads,
            rule_set_commands::rule_set_effective_options,
            rule_set_commands::rule_set_common_status,
            rule_set_commands::rule_set_set_common_enabled,
            rule_set_commands::rule_set_set_common_binding,
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
            gui_core_commands::gui_network_probe,
            gui_core_commands::gui_log_paths,
            gui_core_commands::gui_debug_storage_summary,
            gui_core_commands::gui_clear_debug_storage,
            gui_core_commands::gui_export_diagnostics,
            tray_update_status,
        ])
        // ── Phase 5: Runtime — tray, kernel lifecycle, window ──
        .setup(|app| {
            crate::services::file_logger::line("runtime: setup begin");
            let ipc_observer =
                std::sync::Arc::new(crate::services::ipc_observability::IpcLogObserver::default());
            let observer_app = app.handle().clone();
            let installed = crate::models::debug::install_debug_frame_observer(
                std::sync::Arc::new(move |frame| {
                    let state = observer_app.state::<AppState>();
                    ipc_observer.observe(state.inner(), frame);
                }),
            );
            if !installed {
                crate::services::file_logger::line(
                    "runtime: IPC debug frame observer was already installed",
                );
            }
            if let Err(error) = network_probe::start_host_network_monitor(app.handle()) {
                crate::services::file_logger::line(&format!(
                    "runtime: host network monitor unavailable: {}",
                    error.message
                ));
            }
            // A GUI lifetime owns exactly one kernel child. Never probe or
            // adopt a process from an earlier GUI lifetime; the private IPC
            // endpoint and inherited stdin pipe form the ownership contract.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let _operation = state.proxy_config_operation().lock().await;
                    let core_config = state
                        .app_config()
                        .lock()
                        .map(|c| c.core.clone())
                        .unwrap_or_default();
                    if !core_config.auto_start {
                        crate::services::file_logger::line(
                            "auto_start disabled, not starting kernel",
                        );
                        return;
                    }

                    let app_handle_start = app_handle.clone();
                    let start_result = tauri::async_runtime::spawn_blocking(move || {
                        let state = app_handle_start.state::<AppState>();
                        core_process::start(app_handle_start.clone(), state)
                    })
                    .await;
                    // `connect` serializes on the same operation mutex, so
                    // release the startup transaction before entering it.
                    drop(_operation);

                    match start_result {
                        Ok(Ok(_)) if core_config.auto_connect => {
                            let connect_state = app_handle.state::<AppState>();
                            if let Err(error) = crate::services::gui_connection::connect(
                                app_handle.clone(),
                                connect_state,
                            )
                            .await
                            {
                                crate::services::file_logger::line(&format!(
                                    "failed to auto-connect to managed kernel: {}",
                                    error.message
                                ));
                            }
                        }
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => crate::services::file_logger::line(&format!(
                            "failed to auto-start managed kernel: {}",
                            error.message
                        )),
                        Err(error) => crate::services::file_logger::line(&format!(
                            "managed kernel start task failed: {error}"
                        )),
                    }
                });
            }

            // System tray
            let show_item = tauri::menu::MenuItemBuilder::new("打开 ZNet Sink")
                .id("show")
                .build(app)?;
            let status_item = tauri::menu::MenuItemBuilder::new("状态：正在初始化")
                .id("status")
                .enabled(false)
                .build(app)?;
            let profile_item = tauri::menu::MenuItemBuilder::new("当前配置：读取中")
                .id("profile")
                .enabled(false)
                .build(app)?;
            let toggle_proxy_item = tauri::menu::MenuItemBuilder::new("开启代理")
                .id("toggle_proxy")
                .enabled(false)
                .build(app)?;
            let system_proxy_item = tauri::menu::CheckMenuItemBuilder::new("系统代理")
                .id("toggle_system_proxy")
                .enabled(false)
                .checked(false)
                .build(app)?;
            let tun_item = tauri::menu::CheckMenuItemBuilder::new("TUN 模式")
                .id("toggle_tun")
                .enabled(false)
                .checked(false)
                .build(app)?;
            let restart_core_item = tauri::menu::MenuItemBuilder::new("重启内核")
                .id("restart_core")
                .enabled(false)
                .build(app)?;
            let copy_proxy_env_item = tauri::menu::MenuItemBuilder::new("复制代理环境变量")
                .id("copy_proxy_env")
                .build(app)?;
            #[cfg(target_os = "windows")]
            let open_proxy_terminal_item = tauri::menu::MenuItemBuilder::new("打开终端")
                .id("open_proxy_terminal")
                .build(app)?;
            let settings_item = tauri::menu::MenuItemBuilder::new("设置")
                .id("settings")
                .build(app)?;
            let overview_item = tauri::menu::MenuItemBuilder::new("概览")
                .id("overview")
                .build(app)?;
            let nodes_item = tauri::menu::MenuItemBuilder::new("节点")
                .id("nodes")
                .build(app)?;
            let subscriptions_item = tauri::menu::MenuItemBuilder::new("订阅")
                .id("subscriptions")
                .build(app)?;
            let logs_item = tauri::menu::MenuItemBuilder::new("日志")
                .id("logs")
                .build(app)?;
            let quit_item = tauri::menu::MenuItemBuilder::new("退出")
                .id("quit")
                .build(app)?;

            let proxy_controls = tauri::menu::SubmenuBuilder::new(app, "代理控制")
                .items(&[&system_proxy_item, &tun_item])
                .build()?;
            let shortcuts = tauri::menu::SubmenuBuilder::new(app, "快捷入口")
                .items(&[&overview_item, &nodes_item, &subscriptions_item, &logs_item])
                .build()?;

            #[cfg(target_os = "windows")]
            let tools = tauri::menu::SubmenuBuilder::new(app, "诊断与工具")
                .items(&[
                    &restart_core_item,
                    &copy_proxy_env_item,
                    &open_proxy_terminal_item,
                ])
                .build()?;

            #[cfg(not(target_os = "windows"))]
            let tools = tauri::menu::SubmenuBuilder::new(app, "诊断与工具")
                .items(&[&restart_core_item, &copy_proxy_env_item])
                .build()?;

            let tray_menu = tauri::menu::Menu::with_items(
                app,
                &[
                    &show_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &status_item,
                    &profile_item,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &toggle_proxy_item,
                    &proxy_controls,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &shortcuts,
                    &settings_item,
                    &tools,
                    &tauri::menu::PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ],
            )?;

            // Hold references to the status-dependent items so
            // `tray_update_status` can toggle their enabled state.
            app.manage(TrayMenuItems {
                status: status_item,
                profile: profile_item,
                toggle_proxy: toggle_proxy_item,
                system_proxy: system_proxy_item,
                tun: tun_item,
                restart_core: restart_core_item,
            });

            let _tray_menu = TrayIconBuilder::with_id("main-tray")
                .tooltip("ZNet Sink · 已停止")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" | "overview" => open_main_window_route(app, "overview", None),
                    "toggle_proxy" => emit_tray_action(app, "toggle_proxy"),
                    "toggle_system_proxy" => emit_tray_action(app, "toggle_system_proxy"),
                    "toggle_tun" => emit_tray_action(app, "toggle_tun"),
                    "restart_core" => emit_tray_action(app, "restart_core"),
                    "nodes" => open_main_window_route(app, "nodes", None),
                    "subscriptions" => open_main_window_route(app, "subscriptions", None),
                    "logs" => open_main_window_route(app, "logs", None),
                    "copy_proxy_env" => tray_copy_proxy_environment(app),
                    #[cfg(target_os = "windows")]
                    "open_proxy_terminal" => tray_open_proxy_terminal(app.clone()),
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
            crate::services::rule_set::spawn_auto_update_scheduler(app.handle().clone());

            // Spawn the traffic sampler so the overview chart updates live —
            // the kernel doesn't push traffic events on its own (TODO P5).
            crate::services::traffic_sampler::spawn(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    let exit_shutdown_stage = shutdown_stage.clone();
    app.run(move |app_handle, event| {
        if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
            use std::sync::atomic::Ordering;

            match exit_shutdown_stage.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {
                    api.prevent_exit();
                    let cleanup_app = app_handle.clone();
                    let cleanup_stage = exit_shutdown_stage.clone();
                    tauri::async_runtime::spawn(async move {
                        core_process::shutdown_managed_runtime(cleanup_app.clone()).await;
                        cleanup_stage.store(2, Ordering::SeqCst);
                        cleanup_app.exit(code.unwrap_or(0));
                    });
                }
                Err(1) => {
                    // Ignore repeated quit requests until TUN and the managed
                    // child have both finished their first cleanup attempt.
                    api.prevent_exit();
                }
                Err(_) => {
                    // Stage 2 was set immediately before our own final exit.
                }
            }
        }
    });

    // ── Shutdown: runs after Tauri event loop exits ──
    lifecycle.shutdown();
}

#[cfg(test)]
mod tests {
    use super::{proxy_environment_command, proxy_environment_no_proxy, tray_presentation};
    use crate::models::app_config::default_proxy_bypass;

    #[test]
    fn copied_proxy_environment_uses_http_and_socks_endpoints() {
        let command = proxy_environment_command("127.0.0.1", 7890, &default_proxy_bypass());

        assert!(command.contains("HTTP_PROXY"));
        assert!(command.contains("http://127.0.0.1:7890"));
        assert!(command.contains("ALL_PROXY"));
        assert!(command.contains("socks5h://127.0.0.1:7890"));
        assert!(command.contains("NO_PROXY"));
        assert!(command.contains("192.168.0.0/16"));
        assert!(command.contains("10.0.0.0/8"));
    }

    #[test]
    fn terminal_no_proxy_converts_windows_lan_wildcards_to_cidr() {
        let bypass = vec![
            "<local>".to_string(),
            "127.*".to_string(),
            "[::1]".to_string(),
            "172.16.*".to_string(),
            "192.168.*".to_string(),
            "intranet.example".to_string(),
        ];

        assert_eq!(
            proxy_environment_no_proxy(&bypass),
            "localhost,127.0.0.0/8,::1,172.16.0.0/16,192.168.0.0/16,intranet.example"
        );
    }

    #[test]
    fn lite_tray_treats_partial_capture_as_an_active_session() {
        let presentation = tray_presentation("lite", true, false, true);

        assert_eq!(presentation.status_label, "TUN 运行中");
        assert_eq!(presentation.action_label, "关闭代理");
        assert!(!presentation.individual_controls_enabled);
    }

    #[test]
    fn pro_tray_keeps_tun_and_system_proxy_independent() {
        let presentation = tray_presentation("pro", true, false, true);

        assert_eq!(presentation.status_label, "TUN 运行中");
        assert_eq!(presentation.action_label, "开启代理");
        assert!(presentation.individual_controls_enabled);
    }

    #[test]
    fn tray_reports_combined_capture_and_stopped_states() {
        let combined = tray_presentation("pro", true, true, true);
        let stopped = tray_presentation("lite", false, false, false);

        assert_eq!(combined.status_label, "系统代理 + TUN");
        assert_eq!(combined.action_label, "关闭代理");
        assert_eq!(stopped.status_label, "已停止");
        assert_eq!(stopped.action_label, "开启代理");
    }
}
