//! Application composition and service lifecycle, separate from platform UI.

mod commands;

use crate::lifecycle::{self, phases};
use crate::services::{core_process, network_probe, system_proxy_guard};
use crate::state::app_state::AppState;
use tauri::Manager;

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
    let builder = tauri::Builder::default()
        .manage(app_state)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());
    // ── Phase 4: Register commands ──
    let app = commands::register(builder)
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

            crate::desktop::tray::install(app)?;

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
