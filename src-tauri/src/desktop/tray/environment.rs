use crate::state::app_state::AppState;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "windows")]
use crate::services::{core_process, local_proxy};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NEW_CONSOLE: u32 = 0x00000010;

pub(super) fn proxy_environment_no_proxy(bypass: &[String]) -> String {
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

pub(super) fn proxy_environment_command(host: &str, port: u16, bypass: &[String]) -> String {
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

pub(super) fn tray_copy_proxy_environment(app: &tauri::AppHandle) {
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
pub(super) fn tray_open_proxy_terminal(app: tauri::AppHandle) {
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
