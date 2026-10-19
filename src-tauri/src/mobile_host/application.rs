//! Mobile UI composition. No UI-owned child, TUN, scheduler or exit teardown.
//! Native forwarding must be owned by VpnService/PacketTunnelProvider when
//! the complete runtime bridge is available.
use std::sync::{Arc, Mutex};
use tauri::Manager;

use crate::lifecycle::OnPhase;

pub(crate) fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_znet_native_io::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    let app = crate::application::commands::register(builder)
        .setup(|app| {
            // Android/iOS private app container is resolved by the native
            // host. Never inherit HOME, desktop paths or a working directory.
            crate::services::install_mobile_data_dir(app.path().app_data_dir()?)
                .map_err(|error| std::io::Error::other(error.message))?;
            crate::services::file_logger::init();
            if let Err(error) = crate::services::data_dir().and_then(|dir| {
                crate::models::debug::initialize_capture_recovery(
                    &dir.join("capture-recovery.json"),
                )
            }) {
                crate::services::file_logger::line(&error.message);
            }
            let data = Arc::new(Mutex::new(None));
            crate::lifecycle::phases::ConfigPhase { data: data.clone() }
                .run()
                .map_err(|error| std::io::Error::other(error.message))?;
            let startup = data
                .lock()
                .expect("mobile startup lock")
                .take()
                .expect("mobile config phase should provide startup data");
            app.manage(crate::state::app_state::AppState::with_domain_data(
                startup.app_config,
                startup.domain_data.proxy_configs,
                startup.domain_data.subscriptions,
                startup.domain_data.rule_sets,
                startup.logs,
            ));
            // UI disappearance does not own service shutdown. Do not launch
            // desktop subscription/probe timers and call them mobile jobs.
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building mobile UI application");
    app.run(|app, event| {
        // Only trusted main-window lifecycle events authorize release checks.
        // This owns metadata GETs, never VPN forwarding or installation.
        match &event {
            tauri::RunEvent::Ready => crate::services::plugins::resume_native(app),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Resumed,
                ..
            } if label == "main" => crate::services::plugins::resume_native(app),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Suspended,
                ..
            } if label == "main" => crate::services::plugins::suspend_native(app),
            tauri::RunEvent::Exit => crate::services::plugins::shutdown_native(app),
            tauri::RunEvent::ExitRequested { .. } => crate::services::plugins::suspend_native(app),
            _ => {}
        }
        match &event {
            tauri::RunEvent::Ready => crate::services::mobile_release_scheduler::resume(app),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Resumed,
                ..
            } if label == "main" => crate::services::mobile_release_scheduler::resume(app),
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Suspended,
                ..
            } if label == "main" => crate::services::mobile_release_scheduler::suspend(app),
            tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. } => {
                crate::services::mobile_release_scheduler::suspend(app)
            }
            _ => {}
        }
        match event {
            tauri::RunEvent::Ready
            | tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Resumed,
                ..
            } => {
                crate::services::mobile_subscription_scheduler::resume(app);
            }
            tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Suspended,
                ..
            }
            | tauri::RunEvent::Exit
            | tauri::RunEvent::ExitRequested { .. } => {
                crate::services::mobile_subscription_scheduler::suspend(app);
            }
            _ => {}
        }
    });
}
