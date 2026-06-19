use crate::services::common;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

/// Snapshot of the user's original system-proxy configuration, captured
/// immediately before the GUI overrides it. Persisted inside the proxy
/// marker so a later "disable" can *restore* the user's settings instead of
/// blanking them — which is what previously destroyed users' pre-existing
/// proxies (e.g. their own `127.0.0.1:1080`) whenever the kernel stopped or
/// the app exited.
///
/// The Windows-specific `override_bypass` (`ProxyOverride`) and
/// `auto_config_url` (`AutoConfigURL`) fields use `None` to mean "the value
/// was absent before we touched anything". Because the GUI never creates
/// those values, `None` also means "leave untouched" on restore.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyBackup {
    /// Whether the OS proxy was enabled before the GUI touched it.
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// Windows `ProxyOverride` (bypass list), e.g. `<local>;192.168.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_bypass: Option<String>,
    /// Windows `AutoConfigURL` (PAC script), if configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_config_url: Option<String>,
}

pub fn enable(host: &str, port: u16) -> AppResult<SystemProxyStatus> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::invalid_argument("proxy host must not be empty"));
    }
    if port == 0 {
        return Err(AppError::invalid_argument("proxy port must not be zero"));
    }

    set_proxy_platform(host, port, true)?;

    Ok(SystemProxyStatus {
        enabled: true,
        host: host.to_string(),
        port,
    })
}

/// Blank the system proxy unconditionally.
///
/// This is a **destructive** operation — it discards whatever proxy was
/// configured. The GUI lifecycle should normally go through
/// [`crate::services::system_proxy_guard`], which captures a [`ProxyBackup`]
/// on enable and [`restore`]s it on disable, so the user's original settings
/// are recovered instead of being wiped.
pub fn disable() -> AppResult<SystemProxyStatus> {
    set_proxy_platform("", 0, false)?;

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
    })
}

pub fn status() -> AppResult<SystemProxyStatus> {
    status_platform()
}

/// Read the current OS proxy settings into a [`ProxyBackup`] so they can be
/// restored later. Must be called *before* overwriting anything.
pub fn capture_backup() -> AppResult<ProxyBackup> {
    capture_backup_platform()
}

/// Restore the OS proxy settings from a [`ProxyBackup`] — the inverse of
/// [`capture_backup`]. Used by the proxy guard instead of the destructive
/// [`disable`] so the user's original configuration is recovered.
pub fn restore(backup: &ProxyBackup) -> AppResult<()> {
    restore_platform(backup)
}

// ── macOS ──

#[cfg(target_os = "macos")]
fn set_proxy_platform(host: &str, port: u16, enable: bool) -> AppResult<()> {
    let services = active_network_services()?;
    if services.is_empty() {
        return Err(AppError::internal(
            "no active network service found; cannot configure system proxy",
        ));
    }

    for service in &services {
        if enable {
            run_networksetup(&["-setwebproxy", service, host, &port.to_string()])?;
            run_networksetup(&["-setsecurewebproxy", service, host, &port.to_string()])?;
        } else {
            run_networksetup(&["-setwebproxystate", service, "off"])?;
            run_networksetup(&["-setsecurewebproxystate", service, "off"])?;
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let services = active_network_services()?;
    for service in &services {
        if let Ok(output) = run_networksetup_output(&["-getwebproxy", service]) {
            if output.contains("Enabled: Yes") {
                // Extract host and port from output
                let host = extract_prop(&output, "Server:")
                    .unwrap_or("127.0.0.1")
                    .to_string();
                let port: u16 = extract_prop(&output, "Port:")
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                return Ok(SystemProxyStatus {
                    enabled: true,
                    host,
                    port,
                });
            }
        }
    }

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
    })
}

#[cfg(target_os = "macos")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let status = status_platform()?;
    Ok(ProxyBackup {
        enabled: status.enabled,
        host: status.host,
        port: status.port,
        override_bypass: None,
        auto_config_url: None,
    })
}

#[cfg(target_os = "macos")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    if backup.enabled {
        set_proxy_platform(&backup.host, backup.port, true)
    } else {
        set_proxy_platform("", 0, false)
    }
}

#[cfg(target_os = "macos")]
fn active_network_services() -> AppResult<Vec<String>> {
    // List hardware ports to find active network services
    let output = common::background_command("networksetup")
        .args(["-listallhardwareports"])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();
    let mut lines = stdout.lines().peekable();

    while let Some(line) = lines.next() {
        if line.starts_with("Hardware Port:") {
            let service_name = line.trim_start_matches("Hardware Port:").trim();
            // Skip disabled/inactive ports - check next line for Device
            if let Some(device_line) = lines.next() {
                if device_line.contains("Device:") {
                    let device = device_line.trim_start_matches("Device:").trim();
                    // Only include real network interfaces (skip Bluetooth, Thunderbolt bridge, etc.)
                    if device.starts_with("en") || device.starts_with("wl") {
                        services.push(service_name.to_string());
                    }
                }
            }
        }
    }

    Ok(services)
}

#[cfg(target_os = "macos")]
fn run_networksetup(args: &[&str]) -> AppResult<()> {
    let output = common::background_command("networksetup")
        .args(args)
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "networksetup failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_networksetup_output(args: &[&str]) -> AppResult<String> {
    let output = common::background_command("networksetup")
        .args(args)
        .output()
        .map_err(|e| AppError::internal(format!("failed to run networksetup: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn extract_prop<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .lines()
        .find(|line| line.trim().starts_with(key))
        .and_then(|line| line.trim().strip_prefix(key))
        .map(|v| v.trim())
}

// ── Windows ──

#[cfg(target_os = "windows")]
const INTERNET_SETTINGS_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[cfg(target_os = "windows")]
fn set_proxy_platform(host: &str, port: u16, enable: bool) -> AppResult<()> {
    write_internet_setting_dword("ProxyEnable", if enable { 1 } else { 0 })?;

    // Set ProxyServer via registry
    if enable {
        write_internet_setting_sz("ProxyServer", &format!("{host}:{port}"))?;
    }

    notify_settings_changed();
    Ok(())
}

#[cfg(target_os = "windows")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let enabled = query_internet_setting("ProxyEnable")
        .map(|v| v.trim() == "0x1")
        .unwrap_or(false);

    if enabled {
        let server = query_internet_setting("ProxyServer").unwrap_or_default();
        let (host, port) = parse_server(&server);
        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
        })
    }
}

#[cfg(target_os = "windows")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let enabled = query_internet_setting("ProxyEnable")
        .map(|v| v.trim() == "0x1")
        .unwrap_or(false);
    let server = query_internet_setting("ProxyServer").unwrap_or_default();
    let (host, port) = parse_server(&server);
    Ok(ProxyBackup {
        enabled,
        host,
        port,
        override_bypass: query_internet_setting("ProxyOverride"),
        auto_config_url: query_internet_setting("AutoConfigURL"),
    })
}

#[cfg(target_os = "windows")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    // ProxyEnable — always written so it reflects the original state.
    write_internet_setting_dword("ProxyEnable", if backup.enabled { 1 } else { 0 })?;

    // ProxyServer — restore the original value if one existed; otherwise
    // delete the value so the GUI's own server doesn't linger (it would be
    // inert because ProxyEnable is off, but we keep the registry clean).
    if !backup.host.is_empty() {
        write_internet_setting_sz("ProxyServer", &format!("{}:{}", backup.host, backup.port))?;
    } else {
        delete_internet_setting("ProxyServer");
    }

    // ProxyOverride / AutoConfigURL — only restore when the user actually
    // had them. The GUI never creates these values, so `None` means they
    // were absent and untouched; leaving them alone is correct.
    if let Some(bypass) = &backup.override_bypass {
        write_internet_setting_sz("ProxyOverride", bypass)?;
    }
    if let Some(url) = &backup.auto_config_url {
        write_internet_setting_sz("AutoConfigURL", url)?;
    }

    notify_settings_changed();
    Ok(())
}

/// Query a single value from the Internet Settings registry key.
/// Returns `None` if the value is absent or unreadable.
#[cfg(target_os = "windows")]
fn query_internet_setting(value_name: &str) -> Option<String> {
    let output = common::background_command("reg")
        .args(["query", INTERNET_SETTINGS_KEY, "/v", value_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // A matching line looks like:
    //   "    ProxyServer    REG_SZ    127.0.0.1:1080"
    //   "    ProxyEnable    REG_DWORD    0x1"
    for line in stdout.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(value_name) else {
            continue;
        };
        let rest = rest.trim_start();
        // rest = "REG_SZ    <value>" — skip the type token to get the value.
        let kind_end = rest.find(' ')?;
        return Some(rest[kind_end..].trim().to_string());
    }
    None
}

#[cfg(target_os = "windows")]
fn write_internet_setting_dword(value_name: &str, value: u32) -> AppResult<()> {
    let output = common::background_command("reg")
        .args([
            "add",
            INTERNET_SETTINGS_KEY,
            "/v",
            value_name,
            "/t",
            "REG_DWORD",
            "/d",
            &value.to_string(),
            "/f",
        ])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run reg.exe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "failed to set Windows {}: {}",
            value_name,
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn write_internet_setting_sz(value_name: &str, value: &str) -> AppResult<()> {
    let output = common::background_command("reg")
        .args([
            "add",
            INTERNET_SETTINGS_KEY,
            "/v",
            value_name,
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ])
        .output()
        .map_err(|e| AppError::internal(format!("failed to run reg.exe: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "failed to set Windows {}: {}",
            value_name,
            stderr.trim()
        )));
    }
    Ok(())
}

/// Best-effort deletion of a single value. Silently ignored if the value
/// is absent (which is the common case we want to tolerate on restore).
#[cfg(target_os = "windows")]
fn delete_internet_setting(value_name: &str) {
    let _ = common::background_command("reg")
        .args(["delete", INTERNET_SETTINGS_KEY, "/v", value_name, "/f"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(target_os = "windows")]
fn notify_settings_changed() {
    use std::ptr;
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_PROXY_SETTINGS_CHANGED, INTERNET_OPTION_REFRESH,
    };

    unsafe {
        InternetSetOptionW(
            ptr::null(),
            INTERNET_OPTION_PROXY_SETTINGS_CHANGED,
            ptr::null(),
            0,
        );
        InternetSetOptionW(ptr::null(), INTERNET_OPTION_REFRESH, ptr::null(), 0);
    }
}

#[cfg(target_os = "windows")]
fn parse_server(server: &str) -> (String, u16) {
    server
        .split_once(':')
        .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0)))
        .unwrap_or_default()
}

// ── Linux ──

#[cfg(target_os = "linux")]
fn set_proxy_platform(host: &str, port: u16, enable: bool) -> AppResult<()> {
    let mode = if enable { "manual" } else { "none" };
    let proxy_url = if enable {
        format!("http://{host}:{port}/")
    } else {
        String::new()
    };

    // Try gsettings (GNOME)
    let gsettings_result = common::background_command("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", mode])
        .output();

    if gsettings_result.is_ok() && enable {
        let _ = common::background_command("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "host", host])
            .output();
        let _ = common::background_command("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.http",
                "port",
                &port.to_string(),
            ])
            .output();
        let _ = common::background_command("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", host])
            .output();
        let _ = common::background_command("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.https",
                "port",
                &port.to_string(),
            ])
            .output();
    }

    // Also set environment variables for CLI tools
    if enable {
        std::env::set_var("http_proxy", &proxy_url);
        std::env::set_var("https_proxy", &proxy_url);
        std::env::set_var("HTTP_PROXY", &proxy_url);
        std::env::set_var("HTTPS_PROXY", &proxy_url);
    }

    gsettings_result
        .map(|_| ())
        .map_err(|e| AppError::internal(format!("failed to configure Linux proxy: {e}")))
}

#[cfg(target_os = "linux")]
fn status_platform() -> AppResult<SystemProxyStatus> {
    let output = common::background_command("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
        .map_err(|e| AppError::internal(format!("failed to query Linux proxy: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let enabled = stdout.contains("manual");

    if enabled {
        let host_output = common::background_command("gsettings")
            .args(["get", "org.gnome.system.proxy.http", "host"])
            .output()
            .unwrap_or_else(|_| output.clone());
        let host = String::from_utf8_lossy(&host_output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string();

        let port_output = common::background_command("gsettings")
            .args(["get", "org.gnome.system.proxy.http", "port"])
            .output()
            .unwrap_or_else(|_| output.clone());
        let port: u16 = String::from_utf8_lossy(&port_output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);

        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
        })
    }
}

#[cfg(target_os = "linux")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let status = status_platform()?;
    Ok(ProxyBackup {
        enabled: status.enabled,
        host: status.host,
        port: status.port,
        override_bypass: None,
        auto_config_url: None,
    })
}

#[cfg(target_os = "linux")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    if backup.enabled {
        set_proxy_platform(&backup.host, backup.port, true)
    } else {
        set_proxy_platform("", 0, false)
    }
}
