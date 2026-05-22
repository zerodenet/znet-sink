use std::collections::BTreeSet;
use tauri::State;

use crate::errors::{AppError, AppResult};
use crate::models::app_config::{AppConfig, AppConfigPatch};
use crate::services::app_config_store;
use crate::services::common::{lock, normalize_optional};
use crate::state::app_state::AppState;

const LOG_LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];
const THEMES: &[&str] = &["light", "dark", "system"];
const UI_MODES: &[&str] = &["lite", "pro"];

pub fn get(state: State<'_, AppState>) -> AppResult<AppConfig> {
    Ok(lock(state.app_config(), "app_config")?.clone())
}

pub fn update(state: State<'_, AppState>, patch: AppConfigPatch) -> AppResult<AppConfig> {
    let mut config = lock(state.app_config(), "app_config")?;

    if let Some(core) = patch.core {
        if let Some(kernel) = core.kernel {
            let kernel = kernel.trim().to_ascii_lowercase();
            if kernel.is_empty() {
                return Err(AppError::invalid_argument("core.kernel must not be empty"));
            }
            let current_kernel = config.core.kernel.trim().to_ascii_lowercase();
            if kernel != current_kernel {
                return Err(AppError::invalid_argument("core.kernel is read-only"));
            }
            config.core.kernel = current_kernel;
        }
        if let Some(auto_connect) = core.auto_connect {
            config.core.auto_connect = auto_connect;
        }
        if let Some(auto_start) = core.auto_start {
            config.core.auto_start = auto_start;
        }
        if let Some(executable_path) = core.executable_path {
            config.core.executable_path = normalize_optional(executable_path);
        }
        if let Some(download_url) = core.download_url {
            config.core.download_url = normalize_optional(download_url);
        }
        if let Some(config_path) = core.config_path {
            config.core.config_path = normalize_optional(config_path);
        }
        if let Some(working_dir) = core.working_dir {
            config.core.working_dir = normalize_optional(working_dir);
        }
        if let Some(socket) = core.socket {
            config.core.socket = normalize_optional(socket);
        }
    }

    if let Some(logs) = patch.logs {
        if let Some(level) = logs.level {
            let level = level.trim().to_ascii_lowercase();
            if !LOG_LEVELS.contains(&level.as_str()) {
                return Err(AppError::invalid_argument(
                    "logs.level must be one of trace, debug, info, warn, error",
                ));
            }
            config.logs.level = level;
        }
        if let Some(max_entries) = logs.max_entries {
            if max_entries == 0 {
                return Err(AppError::invalid_argument(
                    "logs.maxEntries must be greater than 0",
                ));
            }
            config.logs.max_entries = max_entries;

            let mut entries = lock(state.logs(), "logs")?;
            if entries.len() > max_entries {
                let remove_count = entries.len() - max_entries;
                entries.drain(0..remove_count);
            }
        }
    }

    if let Some(ui) = patch.ui {
        if let Some(theme) = ui.theme {
            let theme = theme.trim().to_ascii_lowercase();
            if !THEMES.contains(&theme.as_str()) {
                return Err(AppError::invalid_argument(
                    "ui.theme must be one of light, dark, system",
                ));
            }
            config.ui.theme = theme;
        }
        if let Some(ui_mode) = ui.ui_mode {
            let ui_mode = ui_mode.trim().to_ascii_lowercase();
            if !UI_MODES.contains(&ui_mode.as_str()) {
                return Err(AppError::invalid_argument(
                    "ui.uiMode must be one of lite, pro",
                ));
            }
            config.ui.ui_mode = ui_mode;
        }
        if let Some(sidebar_collapsed) = ui.sidebar_collapsed {
            config.ui.sidebar_collapsed = sidebar_collapsed;
        }
        if let Some(hidden_menu_keys) = ui.hidden_menu_keys {
            config.ui.hidden_menu_keys = normalize_menu_keys(hidden_menu_keys);
        }
        if let Some(default_route) = ui.default_route {
            config.ui.default_route = normalize_optional(default_route);
        }
    }

    if let Some(local_proxy) = patch.local_proxy {
        if let Some(host) = local_proxy.host {
            let host = host.trim().to_string();
            if host.is_empty() {
                return Err(AppError::invalid_argument(
                    "localProxy.host must not be empty",
                ));
            }
            config.local_proxy.host = host;
        }
        if let Some(port) = local_proxy.port {
            validate_port(port, "localProxy.port")?;
            config.local_proxy.port = port;
        }
        if let Some(source_proxy_config_id) = local_proxy.source_proxy_config_id {
            config.local_proxy.source_proxy_config_id = normalize_optional(source_proxy_config_id);
        }
    }

    app_config_store::save(&app_config_store::default_config_path()?, &config)?;

    Ok(config.clone())
}

pub fn normalize_menu_keys(keys: Vec<String>) -> Vec<String> {
    keys.into_iter()
        .filter_map(|key| {
            let key = key.trim().to_ascii_lowercase();
            (!key.is_empty() && key != "settings").then_some(key)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn validate_port(port: u16, field: &'static str) -> AppResult<()> {
    if port == 0 {
        return Err(AppError::invalid_argument(format!(
            "{field} must be between 1 and 65535"
        )));
    }
    Ok(())
}
