pub use znet_client_core as client_core;
pub mod commands;
pub mod config;
pub mod configuration;
pub mod errors;
pub mod events;
pub mod kernel;
pub mod lifecycle;
pub mod models;
pub mod services;
pub mod state;

mod application;
mod desktop;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    application::run();
}

pub mod runtime_host;

pub mod capture;
