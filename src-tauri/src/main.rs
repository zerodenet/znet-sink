// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "macos")]
    if let Some(result) = gui_lib::services::macos_privilege::run_if_requested() {
        if let Err(error) = result {
            eprintln!("macOS privileged helper failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    gui_lib::run()
}
