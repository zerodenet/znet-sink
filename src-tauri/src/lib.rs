pub use znet_client_core as client_core;
pub mod commands;
pub mod config;
pub mod configuration;
pub mod errors;
pub mod events;
pub mod kernel;
pub(crate) mod license_inventory;
pub mod lifecycle;
pub mod models;
pub mod services;
pub mod state;

mod application;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop;
pub mod mobile_host;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    application::run();
}

pub mod runtime_host;

pub mod capture;
