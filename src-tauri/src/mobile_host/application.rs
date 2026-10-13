//! Mobile UI composition. No UI-owned child, TUN, scheduler or exit teardown.
//! Native forwarding must be owned by VpnService/PacketTunnelProvider when
//! the complete runtime bridge is available.
use std::sync::{Arc, Mutex};
use tauri::Manager;

use crate::lifecycle::OnPhase;

pub(crate) fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    crate::application::commands::register(builder)
        .setup(|app| {
            // Android/iOS private app container is resolved by the native
            // host. Never inherit HOME, desktop paths or a working directory.
            crate::services::install_mobile_data_dir(app.path().app_data_dir()?)
                .map_err(|error| std::io::Error::other(error.message))?;
            crate::services::file_logger::init();
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
        .run(tauri::generate_context!())
        .expect("error while building mobile UI application");
}
