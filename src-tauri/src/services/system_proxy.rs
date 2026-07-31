use crate::services::common;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::errors::{AppError, AppResult};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyStatus {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub socks_enabled: bool,
    #[serde(default)]
    pub socks_host: String,
    #[serde(default)]
    pub socks_port: u16,
}

/// Snapshot of the user's original system-proxy configuration, captured
/// immediately before the GUI overrides it. Persisted inside the proxy
/// marker so a later "disable" can *restore* the user's settings instead of
/// blanking them — which is what previously destroyed users' pre-existing
/// proxies (e.g. their own `127.0.0.1:1080`) whenever the kernel stopped or
/// the app exited.
///
/// Windows keeps the original `ProxyServer` string verbatim because it may
/// contain a protocol map with different endpoints. The other optional
/// Windows fields use `None` to mean the registry value was absent before the
/// GUI touched the system and therefore must be absent again after restore.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyBackup {
    /// Whether the OS proxy was enabled before the GUI touched it.
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub socks_enabled: bool,
    #[serde(default)]
    pub socks_host: String,
    #[serde(default)]
    pub socks_port: u16,
    /// Windows `ProxyServer`, preserved verbatim for lossless restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_server: Option<String>,
    /// Windows `ProxyOverride` (bypass list), e.g. `<local>;192.168.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_bypass: Option<String>,
    /// Windows `AutoConfigURL` (PAC script), if configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_config_url: Option<String>,
}

pub fn enable(host: &str, port: u16) -> AppResult<SystemProxyStatus> {
    enable_with_bypass(
        host,
        port,
        &crate::models::app_config::default_proxy_bypass(),
    )
}

pub fn enable_with_bypass(
    host: &str,
    port: u16,
    bypass: &[String],
) -> AppResult<SystemProxyStatus> {
    let host = host.trim();
    if host.is_empty() {
        return Err(AppError::invalid_argument("proxy host must not be empty"));
    }
    if port == 0 {
        return Err(AppError::invalid_argument("proxy port must not be zero"));
    }

    let socks_enabled = supports_socks5(host, port);
    set_proxy_platform(host, port, true, socks_enabled, bypass)?;

    Ok(SystemProxyStatus {
        enabled: true,
        host: host.to_string(),
        port,
        socks_enabled,
        socks_host: if socks_enabled {
            host.to_string()
        } else {
            String::new()
        },
        socks_port: if socks_enabled { port } else { 0 },
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
    set_proxy_platform("", 0, false, false, &[])?;

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
        socks_enabled: false,
        socks_host: String::new(),
        socks_port: 0,
    })
}

pub fn status() -> AppResult<SystemProxyStatus> {
    status_platform()
}

/// Whether the current platform exposes and has the managed local-network
/// bypass protection. `None` means the platform backend cannot inspect it.
pub fn local_bypass_configured() -> Option<bool> {
    local_bypass_configured_platform()
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

fn supports_socks5(host: &str, port: u16) -> bool {
    let timeout = Duration::from_millis(250);
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    for address in addresses {
        let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else {
            continue;
        };
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        if stream.write_all(&[0x05, 0x01, 0x00]).is_err() {
            continue;
        }
        let mut response = [0u8; 2];
        if stream.read_exact(&mut response).is_ok() && response == [0x05, 0x00] {
            return true;
        }
    }
    false
}

#[cfg(any(target_os = "windows", test))]
fn windows_proxy_server(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

#[cfg(any(target_os = "windows", test))]
fn windows_restore_server(backup: &ProxyBackup) -> Option<String> {
    if let Some(server) = backup
        .raw_server
        .as_deref()
        .map(str::trim)
        .filter(|server| !server.is_empty())
    {
        return Some(server.to_string());
    }
    if backup.host.is_empty() {
        return None;
    }
    if backup.socks_enabled {
        Some(format!(
            "http={}:{};https={}:{};socks={}:{}",
            backup.host,
            backup.port,
            backup.host,
            backup.port,
            backup.socks_host,
            backup.socks_port
        ))
    } else {
        Some(windows_proxy_server(&backup.host, backup.port))
    }
}

// ── macOS ──

#[cfg(target_os = "macos")]
fn set_proxy_platform(
    host: &str,
    port: u16,
    enable: bool,
    socks_enabled: bool,
    _bypass: &[String],
) -> AppResult<()> {
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
            if socks_enabled {
                run_networksetup(&["-setsocksfirewallproxy", service, host, &port.to_string()])?;
            } else {
                run_networksetup(&["-setsocksfirewallproxystate", service, "off"])?;
            }
        } else {
            run_networksetup(&["-setwebproxystate", service, "off"])?;
            run_networksetup(&["-setsecurewebproxystate", service, "off"])?;
            run_networksetup(&["-setsocksfirewallproxystate", service, "off"])?;
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
                let socks = run_networksetup_output(&["-getsocksfirewallproxy", service])
                    .unwrap_or_default();
                let socks_enabled = socks.contains("Enabled: Yes");
                let socks_host = extract_prop(&socks, "Server:").unwrap_or("").to_string();
                let socks_port = extract_prop(&socks, "Port:")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                return Ok(SystemProxyStatus {
                    enabled: true,
                    host,
                    port,
                    socks_enabled,
                    socks_host,
                    socks_port,
                });
            }
        }
    }

    Ok(SystemProxyStatus {
        enabled: false,
        host: String::new(),
        port: 0,
        socks_enabled: false,
        socks_host: String::new(),
        socks_port: 0,
    })
}

#[cfg(target_os = "macos")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let status = status_platform()?;
    Ok(ProxyBackup {
        enabled: status.enabled,
        host: status.host.clone(),
        port: status.port,
        socks_enabled: status.socks_enabled,
        socks_host: status.socks_host,
        socks_port: status.socks_port,
        raw_server: None,
        override_bypass: None,
        auto_config_url: None,
    })
}

#[cfg(target_os = "macos")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    if backup.enabled {
        set_proxy_platform(&backup.host, backup.port, true, backup.socks_enabled, &[])?;
        for service in active_network_services()? {
            if backup.socks_enabled {
                run_networksetup(&[
                    "-setsocksfirewallproxy",
                    &service,
                    &backup.socks_host,
                    &backup.socks_port.to_string(),
                ])?;
            } else {
                run_networksetup(&["-setsocksfirewallproxystate", &service, "off"])?;
            }
        }
        Ok(())
    } else {
        set_proxy_platform("", 0, false, false, &[])
    }
}

#[cfg(target_os = "macos")]
fn local_bypass_configured_platform() -> Option<bool> {
    None
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
fn set_proxy_platform(
    host: &str,
    port: u16,
    enable: bool,
    _socks_enabled: bool,
    bypass: &[String],
) -> AppResult<()> {
    if enable {
        // Windows Settings exposes one manual proxy endpoint. A protocol map
        // such as `http=...;https=...;socks=...` is only partially supported
        // by consumers and is not round-trippable through the Settings UI.
        write_internet_setting_sz("ProxyServer", &windows_proxy_server(host, port))?;

        // Never send loopback or LAN traffic back into the local mixed
        // listener. This also protects adapters that appear after Wi-Fi is
        // enabled while the application proxy is already active.
        if bypass.is_empty() {
            delete_internet_setting("ProxyOverride");
        } else {
            write_internet_setting_sz("ProxyOverride", &bypass.join(";"))?;
        }

        // A PAC script can take precedence over the manual proxy for some
        // clients. The guard has already backed it up, so remove it while our
        // proxy is active and restore it on disconnect.
        delete_internet_setting("AutoConfigURL");

        // Enable last so Windows never observes the old server with the new
        // enabled state during the registry update.
        write_internet_setting_dword("ProxyEnable", 1)?;
    } else {
        write_internet_setting_dword("ProxyEnable", 0)?;
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
        let (socks_host, socks_port) = parse_named_server(&server, "socks");
        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
            socks_enabled: server.to_ascii_lowercase().contains("socks="),
            socks_host,
            socks_port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
            socks_enabled: false,
            socks_host: String::new(),
            socks_port: 0,
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
    let (socks_host, socks_port) = parse_named_server(&server, "socks");
    Ok(ProxyBackup {
        enabled,
        host,
        port,
        socks_enabled: !socks_host.is_empty(),
        socks_host,
        socks_port,
        raw_server: (!server.trim().is_empty()).then_some(server),
        override_bypass: query_internet_setting("ProxyOverride"),
        auto_config_url: query_internet_setting("AutoConfigURL"),
    })
}

#[cfg(target_os = "windows")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    // Restore values before ProxyEnable so clients cannot briefly use a stale
    // server while the original state is being reconstructed.
    if let Some(server) = windows_restore_server(backup) {
        write_internet_setting_sz("ProxyServer", &server)?;
    } else {
        delete_internet_setting("ProxyServer");
    }

    if let Some(bypass) = &backup.override_bypass {
        write_internet_setting_sz("ProxyOverride", bypass)?;
    } else {
        delete_internet_setting("ProxyOverride");
    }

    if let Some(url) = &backup.auto_config_url {
        write_internet_setting_sz("AutoConfigURL", url)?;
    } else {
        delete_internet_setting("AutoConfigURL");
    }

    write_internet_setting_dword("ProxyEnable", if backup.enabled { 1 } else { 0 })?;
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
fn local_bypass_configured_platform() -> Option<bool> {
    let bypass = query_internet_setting("ProxyOverride")?.to_ascii_lowercase();
    Some(bypass.contains("<local>") && bypass.contains("localhost") && bypass.contains("127.*"))
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
    if let Some((_, value)) = server.split(';').find_map(|item| item.split_once('=')) {
        return value
            .split_once(':')
            .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0)))
            .unwrap_or_default();
    }
    server
        .split_once(':')
        .map(|(h, p)| (h.to_string(), p.parse::<u16>().unwrap_or(0)))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn parse_named_server(server: &str, name: &str) -> (String, u16) {
    server
        .split(';')
        .filter_map(|item| item.split_once('='))
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.split_once(':'))
        .map(|(host, port)| (host.to_string(), port.parse().unwrap_or(0)))
        .unwrap_or_default()
}

// ── Linux ──

#[cfg(target_os = "linux")]
fn set_proxy_platform(
    host: &str,
    port: u16,
    enable: bool,
    socks_enabled: bool,
    _bypass: &[String],
) -> AppResult<()> {
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
        if socks_enabled {
            let _ = common::background_command("gsettings")
                .args(["set", "org.gnome.system.proxy.socks", "host", host])
                .output();
            let _ = common::background_command("gsettings")
                .args([
                    "set",
                    "org.gnome.system.proxy.socks",
                    "port",
                    &port.to_string(),
                ])
                .output();
        } else {
            let _ = common::background_command("gsettings")
                .args(["set", "org.gnome.system.proxy.socks", "host", ""])
                .output();
        }
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
        let socks_host_output = common::background_command("gsettings")
            .args(["get", "org.gnome.system.proxy.socks", "host"])
            .output()
            .unwrap_or_else(|_| output.clone());
        let socks_host = String::from_utf8_lossy(&socks_host_output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string();
        let socks_port_output = common::background_command("gsettings")
            .args(["get", "org.gnome.system.proxy.socks", "port"])
            .output()
            .unwrap_or_else(|_| output.clone());
        let socks_port: u16 = String::from_utf8_lossy(&socks_port_output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);

        Ok(SystemProxyStatus {
            enabled: true,
            host,
            port,
            socks_enabled: !socks_host.is_empty() && socks_port != 0,
            socks_host,
            socks_port,
        })
    } else {
        Ok(SystemProxyStatus {
            enabled: false,
            host: String::new(),
            port: 0,
            socks_enabled: false,
            socks_host: String::new(),
            socks_port: 0,
        })
    }
}

#[cfg(target_os = "linux")]
fn capture_backup_platform() -> AppResult<ProxyBackup> {
    let status = status_platform()?;
    Ok(ProxyBackup {
        enabled: status.enabled,
        host: status.host.clone(),
        port: status.port,
        socks_enabled: status.socks_enabled,
        socks_host: status.socks_host,
        socks_port: status.socks_port,
        raw_server: None,
        override_bypass: None,
        auto_config_url: None,
    })
}

#[cfg(target_os = "linux")]
fn restore_platform(backup: &ProxyBackup) -> AppResult<()> {
    if backup.enabled {
        set_proxy_platform(&backup.host, backup.port, true, backup.socks_enabled, &[])?;
        if backup.socks_enabled {
            let _ = common::background_command("gsettings")
                .args([
                    "set",
                    "org.gnome.system.proxy.socks",
                    "host",
                    &backup.socks_host,
                ])
                .output();
            let _ = common::background_command("gsettings")
                .args([
                    "set",
                    "org.gnome.system.proxy.socks",
                    "port",
                    &backup.socks_port.to_string(),
                ])
                .output();
        }
        Ok(())
    } else {
        set_proxy_platform("", 0, false, false, &[])
    }
}

#[cfg(target_os = "linux")]
fn local_bypass_configured_platform() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::{windows_proxy_server, windows_restore_server, ProxyBackup};

    #[test]
    fn windows_manual_proxy_uses_settings_compatible_endpoint() {
        assert_eq!(windows_proxy_server("127.0.0.1", 7890), "127.0.0.1:7890");
    }

    #[test]
    fn windows_restore_preserves_protocol_map_verbatim() {
        let backup = ProxyBackup {
            raw_server: Some(
                "http=127.0.0.1:8080;https=127.0.0.1:8443;socks=127.0.0.1:1080"
                    .to_string(),
            ),
            ..ProxyBackup::default()
        };

        assert_eq!(
            windows_restore_server(&backup).as_deref(),
            Some("http=127.0.0.1:8080;https=127.0.0.1:8443;socks=127.0.0.1:1080")
        );
    }

    #[test]
    fn windows_restore_supports_legacy_markers() {
        let backup = ProxyBackup {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 1080,
            ..ProxyBackup::default()
        };

        assert_eq!(
            windows_restore_server(&backup).as_deref(),
            Some("127.0.0.1:1080")
        );
    }
}
