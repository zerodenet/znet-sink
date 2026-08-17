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
    let start = std::time::Instant::now();
    // Validate and persist a detached snapshot. Mutating the shared config as
    // fields are validated can leave a half-applied patch in memory when a
    // later field is invalid or the disk write fails.
    let mut config = lock(state.app_config(), "app_config")?.clone();

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
        if let Some(cleanup_proxy_on_exit) = core.cleanup_proxy_on_exit {
            config.core.cleanup_proxy_on_exit = cleanup_proxy_on_exit;
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
        if let Some(network_probe_urls) = core.network_probe_urls {
            config.core.network_probe_urls = normalize_network_probe_urls(network_probe_urls)?;
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
        if let Some(bypass) = local_proxy.bypass {
            config.local_proxy.bypass = normalize_proxy_bypass(bypass)?;
        }
    }

    if let Some(tun) = patch.tun {
        if let Some(name) = tun.name {
            config.tun.name = normalize_optional(name);
        }
        if let Some(addr) = tun.addr {
            let addr = addr.trim().to_string();
            if addr.is_empty() {
                return Err(AppError::invalid_argument("tun.addr must not be empty"));
            }
            config.tun.addr = addr;
        }
        if let Some(mask) = tun.mask {
            let mask = mask.trim().to_string();
            if mask.is_empty() {
                return Err(AppError::invalid_argument("tun.mask must not be empty"));
            }
            config.tun.mask = mask;
        }
        if let Some(secondary_addr) = tun.secondary_addr {
            config.tun.secondary_addr = normalize_optional(secondary_addr);
        }
        if let Some(tag) = tun.tag {
            let tag = tag.trim().to_string();
            if tag.is_empty() {
                return Err(AppError::invalid_argument("tun.tag must not be empty"));
            }
            config.tun.tag = tag;
        }
        if let Some(mtu) = tun.mtu {
            if mtu < 576 {
                return Err(AppError::invalid_argument("tun.mtu must be at least 576"));
            }
            config.tun.mtu = mtu;
        }
        if let Some(dual_stack) = tun.dual_stack {
            config.tun.dual_stack = dual_stack;
        }
        if let Some(dns_hijack) = tun.dns_hijack {
            config.tun.dns_hijack = dns_hijack;
        }
    }

    if let Some(routing) = patch.routing {
        if let Some(inject_common_rules) = routing.inject_common_rules {
            config.routing.inject_common_rules = inject_common_rules;
        }
    }

    if let Some(url_test) = patch.url_test {
        if let Some(tolerance_ms) = url_test.tolerance_ms {
            config.url_test.tolerance_ms = tolerance_ms;
        }
    }

    replace(state.inner(), config.clone())?;

    eprintln!("[ZNet] app_config_update: took {:?}", start.elapsed(),);

    Ok(config)
}

fn normalize_proxy_bypass(values: Vec<String>) -> AppResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for value in values {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.contains([';', '\r', '\n']) {
            return Err(AppError::invalid_argument(
                "localProxy.bypass entries must not contain semicolons or newlines",
            ));
        }
        let key = value.to_ascii_lowercase();
        if seen.insert(key) {
            normalized.push(value.to_string());
        }
    }
    Ok(normalized)
}

pub(crate) fn replace(state: &AppState, config: AppConfig) -> AppResult<()> {
    app_config_store::save(&app_config_store::default_config_path()?, &config)?;
    *lock(state.app_config(), "app_config")? = config.clone();

    let mut entries = lock(state.logs(), "logs")?;
    if entries.len() > config.logs.max_entries {
        let remove_count = entries.len() - config.logs.max_entries;
        entries.drain(0..remove_count);
    }
    Ok(())
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

pub fn normalize_network_probe_urls(urls: Vec<String>) -> AppResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for raw in urls {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed = reqwest::Url::parse(trimmed).map_err(|error| {
            AppError::invalid_argument(format!(
                "core.networkProbeUrls contains an invalid URL `{trimmed}`: {error}"
            ))
        })?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::invalid_argument(format!(
                "core.networkProbeUrls only supports http(s) URLs: {trimmed}"
            )));
        }

        if parsed.host_str().is_none() {
            return Err(AppError::invalid_argument(format!(
                "core.networkProbeUrls is missing a host: {trimmed}"
            )));
        }

        let normalized_url = parsed.to_string();
        if seen.insert(normalized_url.clone()) {
            normalized.push(normalized_url);
        }
    }

    if normalized.is_empty() {
        return Err(AppError::invalid_argument(
            "core.networkProbeUrls must contain at least one http(s) URL",
        ));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{normalize_network_probe_urls, normalize_proxy_bypass};
    use crate::models::app_config::default_network_probe_urls;

    #[test]
    fn network_probe_urls_trim_and_deduplicate() {
        let urls = normalize_network_probe_urls(vec![
            " https://ipinfo.io/json ".to_string(),
            "https://ipinfo.io/json".to_string(),
            "https://httpbin.org/ip".to_string(),
        ])
        .unwrap();

        assert_eq!(
            urls,
            vec![
                "https://ipinfo.io/json".to_string(),
                "https://httpbin.org/ip".to_string(),
            ]
        );
    }

    #[test]
    fn network_probe_urls_require_http_scheme_and_non_empty_result() {
        assert!(normalize_network_probe_urls(vec!["socks5://127.0.0.1".to_string()]).is_err());
        assert!(normalize_network_probe_urls(vec!["   ".to_string()]).is_err());
        assert!(!default_network_probe_urls().is_empty());
    }

    #[test]
    fn proxy_bypass_entries_trim_and_deduplicate_case_insensitively() {
        let bypass = normalize_proxy_bypass(vec![
            " localhost ".to_string(),
            "LOCALHOST".to_string(),
            "192.168.*".to_string(),
            "".to_string(),
        ])
        .unwrap();

        assert_eq!(
            bypass,
            vec!["localhost".to_string(), "192.168.*".to_string()]
        );
    }

    #[test]
    fn proxy_bypass_entries_reject_platform_separators() {
        assert!(normalize_proxy_bypass(vec!["localhost;example.com".to_string()]).is_err());
        assert!(normalize_proxy_bypass(vec!["localhost\nexample.com".to_string()]).is_err());
    }
}
