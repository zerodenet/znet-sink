pub mod app_config;
pub mod app_config_store;
pub mod capability;
pub(crate) mod common;
pub mod core_config;
pub mod core_events;
pub mod core_process;
pub mod domain_store;
pub mod gui_connection;
pub mod gui_events;
pub mod gui_self_test;
pub mod interaction_mode;
pub mod kernel_manager;
pub mod local_proxy;
pub mod log_store;
pub mod logs;
pub mod probe;
pub mod proxy_config;
pub mod proxy_mode;
pub mod rule_set;
pub mod subscription;
pub mod system_proxy;
pub mod system_proxy_guard;

#[cfg(test)]
mod proxy_mode_tests;

use crate::errors::{AppError, AppResult};
use std::path::PathBuf;

/// Single source of truth for the application data directory.
///
/// Resolution order:
/// 1. `ZNET_SINK_DATA_DIR` env var — development override
/// 2. Platform-specific standard path:
///    - Windows: `%APPDATA%/ZNet Sink`
///    - macOS:   `~/.config/znet-sink`
///    - Linux:   `$XDG_CONFIG_HOME/znet-sink` or `~/.config/znet-sink`
///
/// There is NO `current_dir()` fallback — that leaks dev config into
/// production installs. If none of the above paths resolve, this returns
/// an error so callers can decide how to handle the missing directory.
pub(crate) fn data_dir() -> AppResult<PathBuf> {
    if let Some(path) = std::env::var_os("ZNET_SINK_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = std::env::var_os("APPDATA") {
            return Ok(PathBuf::from(app_data).join("ZNet Sink"));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(config_home).join("znet-sink"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".config").join("znet-sink"));
        }
    }

    Err(AppError::internal(
        "cannot determine data directory: set ZNET_SINK_DATA_DIR or ensure APPDATA/HOME is available",
    ))
}
